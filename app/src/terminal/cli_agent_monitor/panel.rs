//! Panel presentation for the CLI agent monitor (single-column list).
//!
//! Builds stable row descriptions consumed by the Workspace header surface.
//! Field set matches the product requirement: agent, project, state, elapsed,
//! review-pending.

use super::session::MonitorSession;
use super::state::MonitorState;
use super::store::MonitorStore;
use super::ui::{PanelRow, compact_indicator_text, panel_rows_from_store};

/// Shipped panel surface: vertical list of agent sessions.
#[derive(Debug, Clone)]
pub struct CliAgentMonitorPanel {
    pub indicator: String,
    pub rows: Vec<PanelRow>,
}

impl CliAgentMonitorPanel {
    pub fn from_store(
        store: &MonitorStore,
        now_ms: u64,
        terminal_exists: impl Fn(&MonitorSession) -> bool,
    ) -> Self {
        Self {
            indicator: compact_indicator_text(
                store.total_agents(),
                store.pending_review_count(),
            ),
            rows: panel_rows_from_store(store, now_ms, terminal_exists),
        }
    }

    /// One-line debug/log rendering of a row (also used by unit tests for field presence).
    pub fn format_row_line(row: &PanelRow) -> String {
        let pending = if row.review_pending {
            " · pendiente de revisión"
        } else {
            ""
        };
        let mark = if row.show_mark_reviewed {
            " [Marcar como revisado]"
        } else {
            ""
        };
        format!(
            "{} · {} · {} · {}{}{}",
            row.agent, row.project, row.state_label, row.elapsed_label, pending, mark
        )
    }

    pub fn color_for_state(state: MonitorState) -> &'static str {
        state.color_token()
    }
}
