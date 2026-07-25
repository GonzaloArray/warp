//! Presentation helpers for the compact bar indicator and panel rows.

use super::adapters::MonitorAgentKind;
use super::session::MonitorSession;
use super::state::MonitorState;
use super::store::MonitorStore;

/// Compact bar copy: `[icon] N agentes · M para revisar`
pub fn compact_indicator_text(agent_count: usize, pending_review: usize) -> String {
    format!("{agent_count} agentes · {pending_review} para revisar")
}

pub fn compact_indicator_from_store(store: &MonitorStore) -> String {
    compact_indicator_text(store.active_agent_count(), store.pending_review_count())
}

/// One panel row for the vertical session list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelRow {
    pub session_id: String,
    pub agent: String,
    pub project: String,
    pub state: MonitorState,
    pub state_label: String,
    pub elapsed_label: String,
    pub review_pending: bool,
    pub color_token: &'static str,
    /// Show “Marcar como revisado” when terminal association is gone.
    pub show_mark_reviewed: bool,
}

pub fn format_elapsed(elapsed_ms: u64) -> String {
    let secs = elapsed_ms / 1000;
    if secs < 60 {
        return format!("{secs}s");
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins}m");
    }
    let hours = mins / 60;
    format!("{hours}h")
}

pub fn panel_row_from_session(
    session: &MonitorSession,
    now_ms: u64,
    terminal_exists: bool,
) -> PanelRow {
    PanelRow {
        session_id: session.id.clone(),
        agent: session.agent.display_name().to_owned(),
        project: session.project_or_cwd_label(),
        state: session.state,
        state_label: session.state.display_label_es().to_string(),
        elapsed_label: format_elapsed(session.elapsed_ms(now_ms)),
        review_pending: session.is_pending_review(),
        color_token: session.state.color_token(),
        show_mark_reviewed: session.is_pending_review() && !terminal_exists,
    }
}

pub fn panel_rows_from_store(
    store: &MonitorStore,
    now_ms: u64,
    terminal_exists: impl Fn(&MonitorSession) -> bool,
) -> Vec<PanelRow> {
    store
        .sorted_sessions()
        .into_iter()
        .map(|s| panel_row_from_session(s, now_ms, terminal_exists(s)))
        .collect()
}

/// Icon glyph / name for the bar (UI maps to Icon enum).
pub fn bar_icon_name() -> &'static str {
    "AgentMonitor"
}

pub fn agent_kind_for_display(kind: &MonitorAgentKind) -> String {
    kind.display_name().to_owned()
}
