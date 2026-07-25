//! Product-path helpers used by Workspace (focus + review + missing terminal).
//!
//! These functions are the **shipped** navigation/review API: unit tests drive them
//! directly so the real path is covered without a live desktop.

use super::session::NavigationResult;
use super::state::MonitorState;
use super::store::MonitorStore;

/// Outcome of a user activating a monitor row (click).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationOutcome {
    pub state: MonitorState,
    pub reviewed: bool,
    /// UI should show “Marcar como revisado” affordance after this activation.
    pub needs_explicit_mark: bool,
}

/// Apply navigation result to the store after a focus attempt.
///
/// - `COMPLETED_UNSEEN` + terminal exists + focus ok → `REVIEWED`
/// - `RUNNING` / `WAITING` / `ERROR` + focus ok → **keep** live state (do not suppress finish)
/// - Terminal missing + still `COMPLETED_UNSEEN` → `needs_explicit_mark = true`
/// - Focus failed → keep prior state; not reviewed
pub fn product_activate_session(
    store: &mut MonitorStore,
    session_id: &str,
    terminal_exists: bool,
    focus_succeeded: bool,
    now_ms: u64,
) -> Option<ActivationOutcome> {
    let prior = store.get(session_id)?.state;
    let nav = if !terminal_exists {
        NavigationResult::MissingTerminal
    } else if focus_succeeded {
        NavigationResult::Focused
    } else {
        NavigationResult::Failed
    };
    let state = store.activate_session(session_id, nav, now_ms)?;
    Some(ActivationOutcome {
        reviewed: matches!(state, MonitorState::Reviewed)
            && matches!(prior, MonitorState::CompletedUnseen),
        needs_explicit_mark: matches!(nav, NavigationResult::MissingTerminal)
            && matches!(state, MonitorState::CompletedUnseen),
        state,
    })
}

/// Explicit “Marcar como revisado” product path.
/// Returns `Reviewed` when the mark succeeded (row is then removed from the store).
pub fn product_mark_reviewed(
    store: &mut MonitorStore,
    session_id: &str,
    now_ms: u64,
) -> Option<MonitorState> {
    if store.mark_reviewed(session_id, now_ms) {
        Some(MonitorState::Reviewed)
    } else {
        None
    }
}

/// Parse a stored terminal view id string back to EntityId raw usize.
pub fn parse_terminal_view_id(raw: &str) -> Option<usize> {
    raw.parse().ok()
}
