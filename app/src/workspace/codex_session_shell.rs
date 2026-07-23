//! In-session Codex two-column shell: active subagent nav + detail.
//!
//! Subagents must NOT appear as global-sidebar trees/cards. Navigation and
//! detail live inside the Codex terminal tab only. Pure membership/lifecycle
//! logic lives here so unit tests do not need WarpUI.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::agent_tabs_projection::{AgentTabNode, AgentTabStatus, MonitorNodeId};

/// How long a successful completion stays visible before auto-remove from active.
pub(crate) const COMPLETION_FLASH_MS: u64 = 1500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShellNavTarget {
    Parent,
    Subagent,
    HistoryEntry,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActiveSubagentRow {
    pub child_key: String,
    pub display_name: String,
    pub status: AgentTabStatus,
    pub status_label: String,
    pub task_summary: Option<String>,
    pub activity: Option<String>,
    pub needs_attention: bool,
    /// True while we flash "terminado" before auto-remove.
    pub completion_flash: bool,
    /// Optional elapsed label for nav cards (e.g. "2m 14s").
    pub elapsed_label: Option<String>,
}

/// Full archive of a finished subagent run — must answer what was asked, done, and delivered.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct HistorySubagentEntry {
    /// Stable run id (never reuses child_key alone across re-runs).
    #[serde(default)]
    pub id: String,
    pub child_key: String,
    pub display_name: String,
    #[serde(default = "default_parent_label")]
    pub parent_label: String,
    #[serde(default = "default_platform")]
    pub platform: String,
    #[serde(default)]
    pub status: HistoryStatus,
    pub status_label: String,
    pub result_summary: Option<String>,
    pub task_summary: Option<String>,
    /// Original objective / full task description.
    #[serde(default)]
    pub objective: Option<String>,
    /// Human summary of work performed (not raw logs).
    #[serde(default)]
    pub work_summary: Option<String>,
    /// Important activity lines (timeline-friendly).
    #[serde(default)]
    pub activity_highlights: Vec<String>,
    /// Tools / searches used.
    #[serde(default)]
    pub tools_used: Vec<String>,
    /// Files created/modified/deleted (or explicit "no file changes").
    #[serde(default)]
    pub files_changed: Vec<String>,
    /// Message handed back to the parent agent.
    #[serde(default)]
    pub parent_handoff: Option<String>,
    /// Errors / blocks.
    #[serde(default)]
    pub errors: Vec<String>,
    /// Remaining work.
    #[serde(default)]
    pub pending: Vec<String>,
    #[serde(default)]
    pub started_at_ms: u64,
    pub finished_at_ms: u64,
    /// Technical noise kept out of the main UI.
    #[serde(default)]
    pub technical_details: Vec<String>,
}

fn default_parent_label() -> String {
    "Agente principal".into()
}
fn default_platform() -> String {
    "Codex".into()
}

/// Serializable status for history (AgentTabStatus is not Serialize).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HistoryStatus {
    Working,
    Waiting,
    Blocked,
    #[default]
    Completed,
    Failed,
    Unavailable,
}

impl HistoryStatus {
    pub(crate) fn from_tab(status: AgentTabStatus) -> Self {
        match status {
            AgentTabStatus::Working => Self::Working,
            AgentTabStatus::Waiting => Self::Waiting,
            AgentTabStatus::Blocked => Self::Blocked,
            AgentTabStatus::Completed => Self::Completed,
            AgentTabStatus::Failed => Self::Failed,
            AgentTabStatus::Unavailable => Self::Unavailable,
        }
    }

    pub(crate) fn to_tab(self) -> AgentTabStatus {
        match self {
            Self::Working => AgentTabStatus::Working,
            Self::Waiting => AgentTabStatus::Waiting,
            Self::Blocked => AgentTabStatus::Blocked,
            Self::Completed => AgentTabStatus::Completed,
            Self::Failed => AgentTabStatus::Failed,
            Self::Unavailable => AgentTabStatus::Unavailable,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ShellSelection {
    /// Parent agent (Codex session terminal content).
    Parent,
    /// Active subagent key.
    Active(String),
    /// Finished entry shown only via history (not in active list).
    History(String),
}

impl Default for ShellSelection {
    fn default() -> Self {
        Self::Parent
    }
}

/// UI membership + selection for one Codex parent session.
#[derive(Clone, Default)]
pub(crate) struct CodexSessionShellState {
    pub selection: ShellSelection,
    /// Detail column visible (false = "cerrar vista" while keeping list entry).
    pub detail_visible: bool,
    pub history_open: bool,
    /// Manually removed from active list (UI only).
    pub manually_hidden: HashSet<String>,
    /// Local status overrides (e.g. after user Stop).
    pub status_overrides: HashMap<String, AgentTabStatus>,
    /// When a successful completion was first observed (for flash + auto-remove).
    pub completion_seen_at: HashMap<String, Instant>,
    /// Finished subagents kept for "Ver historial".
    pub history: Vec<HistorySubagentEntry>,
    /// Keys already archived into history (do not re-add to active).
    pub archived: HashSet<String>,
    /// Completed but archive validation failed — stay visible until saved.
    pub pending_archive: HashSet<String>,
    /// Pending confirmation for remove while working.
    pub confirm_remove: Option<String>,
    /// Pending confirmation for stop while working.
    pub confirm_stop: Option<String>,
    /// Pending confirmation to delete one history id.
    pub confirm_delete_history_id: Option<String>,
    /// Pending confirmation to clear entire history.
    pub confirm_clear_history: bool,
    /// Multi-select mode for history bulk delete.
    pub history_multi_select: bool,
    /// Selected history ids for bulk delete (only visible/selected rows).
    pub history_selected_ids: HashSet<String>,
    /// Pending confirm for multi-delete.
    pub confirm_multi_delete: bool,
    /// Show collapsible technical details in the detail panel.
    pub show_technical_details: bool,
    /// History search query (nav filter).
    pub history_query: String,
    /// Structured list filter (errors/files/sort). Query string is mirrored in history_query.
    pub history_filter: HistoryListFilter,
    /// Expanded timeline groups in detail (title keys).
    pub expanded_timeline_groups: HashSet<String>,
    /// Stable key for disk persistence (CLI session id when known).
    pub persistence_key: Option<String>,
    /// Test/prod override for history JSON path (None = default config_local_dir).
    pub history_store_path_override: Option<std::path::PathBuf>,
    pub row_mouse: HashMap<String, warpui::elements::MouseStateHandle>,
    /// Scroll for the nav list (active + history rows).
    pub nav_scroll: warpui::elements::ClippedScrollStateHandle,
    /// Scroll for the center detail panel when content overflows.
    pub detail_scroll: warpui::elements::ClippedScrollStateHandle,
}

impl CodexSessionShellState {
    pub(crate) fn new() -> Self {
        Self {
            selection: ShellSelection::Parent,
            detail_visible: true,
            history_open: false,
            ..Default::default()
        }
    }

    pub(crate) fn row_mouse_state(
        &mut self,
        key: &str,
    ) -> warpui::elements::MouseStateHandle {
        self.row_mouse
            .entry(key.to_string())
            .or_default()
            .clone()
    }

    /// Select parent agent in column 2.
    pub(crate) fn select_parent(&mut self) {
        self.selection = ShellSelection::Parent;
        self.detail_visible = true;
        self.confirm_remove = None;
        self.confirm_stop = None;
    }

    /// Select an active subagent; opens detail column.
    pub(crate) fn select_active(&mut self, child_key: String) {
        self.selection = ShellSelection::Active(child_key);
        self.detail_visible = true;
        self.history_open = false;
        self.confirm_remove = None;
        self.confirm_stop = None;
    }

    /// Close/hide column 2 only — subagent stays in the active list.
    pub(crate) fn close_view(&mut self) {
        self.detail_visible = false;
        self.confirm_remove = None;
        self.confirm_stop = None;
    }

    pub(crate) fn toggle_history(&mut self) {
        self.history_open = !self.history_open;
        if self.history_open {
            // History is separate from active; keep selection unless viewing history entry.
        } else if matches!(self.selection, ShellSelection::History(_)) {
            self.selection = ShellSelection::Parent;
        }
    }

    pub(crate) fn select_history(&mut self, child_key: String) {
        self.selection = ShellSelection::History(child_key);
        self.detail_visible = true;
        self.history_open = true;
    }

    /// Request remove: if working, stash for confirmation; else remove now.
    pub(crate) fn request_remove_from_list(
        &mut self,
        child_key: &str,
        is_working: bool,
    ) -> RemoveOutcome {
        if is_working && self.confirm_remove.as_deref() != Some(child_key) {
            self.confirm_remove = Some(child_key.to_string());
            return RemoveOutcome::NeedsConfirmation;
        }
        self.confirm_remove = None;
        self.remove_from_list(child_key);
        RemoveOutcome::Removed
    }

    pub(crate) fn remove_from_list(&mut self, child_key: &str) {
        self.manually_hidden.insert(child_key.to_string());
        if matches!(
            &self.selection,
            ShellSelection::Active(k) | ShellSelection::History(k) if k == child_key
        ) {
            self.selection = ShellSelection::Parent;
        }
        self.completion_seen_at.remove(child_key);
    }

    /// Request stop only when a real cancel path exists.
    /// Without a provider cancel, this is a no-op (never fakes Failed).
    pub(crate) fn request_stop(
        &mut self,
        child_key: &str,
        cancel_available: bool,
    ) -> StopOutcome {
        if !cancel_available {
            self.confirm_stop = None;
            return StopOutcome::Unavailable;
        }
        if self.confirm_stop.as_deref() != Some(child_key) {
            self.confirm_stop = Some(child_key.to_string());
            return StopOutcome::NeedsConfirmation;
        }
        self.confirm_stop = None;
        // Real cancel was invoked by the caller; reflect stopped status in UI.
        self.status_overrides
            .insert(child_key.to_string(), AgentTabStatus::Failed);
        StopOutcome::Stopped
    }

    /// Whether the product may show "Detener ejecución".
    /// Codex child threads currently have no reliable cancel API from Warp.
    pub(crate) fn stop_action_available() -> bool {
        false
    }

    pub(crate) fn cancel_pending_confirms(&mut self) {
        self.confirm_remove = None;
        self.confirm_stop = None;
        self.confirm_delete_history_id = None;
        self.confirm_clear_history = false;
    }

    pub(crate) fn toggle_technical_details(&mut self) {
        self.show_technical_details = !self.show_technical_details;
    }

    pub(crate) fn set_history_query(&mut self, query: String) {
        self.history_query = query.clone();
        self.history_filter.query = query;
    }

    pub(crate) fn cycle_history_sort(&mut self) {
        self.history_filter.sort = match self.history_filter.sort {
            HistorySort::FinishedNewest => HistorySort::FinishedOldest,
            HistorySort::FinishedOldest => HistorySort::DurationLongest,
            HistorySort::DurationLongest => HistorySort::DurationShortest,
            HistorySort::DurationShortest => HistorySort::FinishedNewest,
        };
    }

    pub(crate) fn toggle_history_errors_only(&mut self) {
        self.history_filter.errors_only = !self.history_filter.errors_only;
    }

    pub(crate) fn toggle_history_files_only(&mut self) {
        self.history_filter.files_changed_only = !self.history_filter.files_changed_only;
    }

    /// Toggle “solo hoy” using local midnight as finished_since_ms.
    pub(crate) fn toggle_history_today_only(&mut self) {
        if self.history_filter.finished_since_ms.is_some() {
            self.history_filter.finished_since_ms = None;
        } else {
            self.history_filter.finished_since_ms = Some(start_of_local_day_ms());
        }
    }

    pub(crate) fn toggle_timeline_group(&mut self, key: &str) {
        if self.expanded_timeline_groups.contains(key) {
            self.expanded_timeline_groups.remove(key);
        } else {
            self.expanded_timeline_groups.insert(key.to_string());
        }
    }

    pub(crate) fn toggle_history_multi_select(&mut self) {
        self.history_multi_select = !self.history_multi_select;
        if !self.history_multi_select {
            self.history_selected_ids.clear();
            self.confirm_multi_delete = false;
        }
    }

    pub(crate) fn toggle_history_id_selected(&mut self, id: &str) {
        if self.history_selected_ids.contains(id) {
            self.history_selected_ids.remove(id);
        } else {
            self.history_selected_ids.insert(id.to_string());
        }
        self.confirm_multi_delete = false;
    }

    pub(crate) fn select_all_visible_history(&mut self, visible_ids: &[String]) {
        for id in visible_ids {
            self.history_selected_ids.insert(id.clone());
        }
        self.confirm_multi_delete = false;
    }

    /// Bulk delete selected; requires confirmation when any selected.
    pub(crate) fn request_delete_selected_history(&mut self) -> MultiDeleteOutcome {
        if self.history_selected_ids.is_empty() {
            return MultiDeleteOutcome::Empty;
        }
        if !self.confirm_multi_delete {
            self.confirm_multi_delete = true;
            return MultiDeleteOutcome::NeedsConfirmation {
                count: self.history_selected_ids.len(),
            };
        }
        let ids: Vec<String> = self.history_selected_ids.iter().cloned().collect();
        let removed = delete_history_ids(self, &ids);
        self.history_selected_ids.clear();
        self.confirm_multi_delete = false;
        self.history_multi_select = false;
        MultiDeleteOutcome::Deleted { count: removed }
    }

    /// Request delete of one history entry (confirmation gate).
    pub(crate) fn request_delete_history(&mut self, id: &str) -> DeleteHistoryOutcome {
        if self.confirm_delete_history_id.as_deref() != Some(id) {
            self.confirm_delete_history_id = Some(id.to_string());
            self.confirm_clear_history = false;
            return DeleteHistoryOutcome::NeedsConfirmation;
        }
        self.confirm_delete_history_id = None;
        let deleted_open = matches!(
            &self.selection,
            ShellSelection::History(k) if self.history.iter().any(|h| h.id == id && h.child_key == *k)
                || self.history.iter().any(|h| h.id == id && (h.id == *k || h.child_key == *k))
        );
        // Also match selection by history id if we store History(id).
        let deleted_open = deleted_open
            || matches!(&self.selection, ShellSelection::History(k) if k == id);
        self.history.retain(|h| h.id != id && h.child_key != id);
        if deleted_open {
            self.selection = ShellSelection::Parent;
            self.detail_visible = true;
        }
        let _ = self.persist_history_now();
        DeleteHistoryOutcome::Deleted { returned_to_parent: deleted_open }
    }

    pub(crate) fn request_clear_history(&mut self) -> ClearHistoryOutcome {
        if self.history.is_empty() {
            return ClearHistoryOutcome::Empty;
        }
        if !self.confirm_clear_history {
            self.confirm_clear_history = true;
            self.confirm_delete_history_id = None;
            return ClearHistoryOutcome::NeedsConfirmation {
                count: self.history.len(),
            };
        }
        let count = self.history.len();
        self.history.clear();
        self.confirm_clear_history = false;
        if matches!(self.selection, ShellSelection::History(_)) {
            self.selection = ShellSelection::Parent;
            self.detail_visible = true;
        }
        let _ = self.persist_history_now();
        ClearHistoryOutcome::Cleared { count }
    }

    /// Persist current history. Returns Err if disk write fails (caller must not drop active).
    pub(crate) fn persist_history_now(&self) -> std::io::Result<()> {
        let Some(key) = self.persistence_key.as_deref() else {
            // No session key yet — treat as not safely persisted.
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "missing persistence_key",
            ));
        };
        let path = self
            .history_store_path_override
            .clone()
            .unwrap_or_else(history_store_path);
        save_history_store_to(&path, key, &self.history)
    }

    /// Retry archiving children stuck in `pending_archive` when richer data / disk recovers.
    pub(crate) fn retry_pending_archives(
        &mut self,
        live: &[LiveChildInput],
    ) -> Vec<String> {
        let mut archived_now = Vec::new();
        let pending: Vec<String> = self.pending_archive.iter().cloned().collect();
        for key in pending {
            let Some(child) = live.iter().find(|c| c.child_key == key) else {
                continue;
            };
            if try_archive_completed(self, child, AgentTabStatus::Completed)
                == ArchiveOutcome::Archived
            {
                archived_now.push(key);
            }
        }
        archived_now
    }

    pub(crate) fn load_persisted_history(&mut self, key: &str) {
        if self.persistence_key.as_deref() == Some(key) && !self.history.is_empty() {
            return;
        }
        self.persistence_key = Some(key.to_string());
        let loaded = load_history_store(key);
        if !loaded.is_empty() {
            // Merge: prefer disk for keys not in memory.
            let mut seen: HashSet<String> = self.history.iter().map(|h| h.id.clone()).collect();
            for entry in loaded {
                if seen.insert(entry.id.clone()) {
                    self.history.push(entry);
                }
            }
            for h in &self.history {
                self.archived.insert(h.child_key.clone());
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeleteHistoryOutcome {
    NeedsConfirmation,
    Deleted { returned_to_parent: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ClearHistoryOutcome {
    Empty,
    NeedsConfirmation { count: usize },
    Cleared { count: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MultiDeleteOutcome {
    Empty,
    NeedsConfirmation { count: usize },
    Deleted { count: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RemoveOutcome {
    NeedsConfirmation,
    Removed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StopOutcome {
    NeedsConfirmation,
    Stopped,
    /// No real cancel path — UI must not fake a stop.
    Unavailable,
}

/// Whether a live status still needs attention in the active list.
pub(crate) fn status_needs_active_slot(status: AgentTabStatus) -> bool {
    match status {
        AgentTabStatus::Working
        | AgentTabStatus::Waiting
        | AgentTabStatus::Blocked
        | AgentTabStatus::Failed
        | AgentTabStatus::Unavailable => true,
        AgentTabStatus::Completed => false,
    }
}

pub(crate) fn status_label_es(status: AgentTabStatus) -> &'static str {
    match status {
        AgentTabStatus::Working => "trabajando",
        AgentTabStatus::Waiting => "esperando",
        AgentTabStatus::Blocked => "bloqueado",
        AgentTabStatus::Completed => "terminado",
        AgentTabStatus::Failed => "error",
        AgentTabStatus::Unavailable => "iniciando",
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Approx local midnight UTC-offset naive: now - (secs since midnight local).
/// Good enough for “solo hoy” filter without chrono dependency here.
fn start_of_local_day_ms() -> u64 {
    let now = now_ms();
    // Use local offset via chrono if available; else fall back to last 24h window.
    #[cfg(feature = "chrono")]
    {
        // not always enabled
    }
    let day_ms = 86_400_000u64;
    now.saturating_sub(now % day_ms)
}

/// Input snapshot of one live child from topology (+ optional rich detail).
#[derive(Clone, Debug)]
pub(crate) struct LiveChildInput {
    pub child_key: String,
    pub display_name: String,
    pub status: AgentTabStatus,
    pub task_summary: Option<String>,
    pub activity: Option<String>,
    pub result_summary: Option<String>,
    pub objective: Option<String>,
    pub work_summary: Option<String>,
    pub activity_highlights: Vec<String>,
    pub tools_used: Vec<String>,
    pub files_changed: Vec<String>,
    pub errors: Vec<String>,
    pub pending: Vec<String>,
    pub technical_details: Vec<String>,
    pub started_at_ms: Option<u64>,
    pub elapsed_label: Option<String>,
}

/// Result of reconciling live topology into active rows + side effects.
#[derive(Clone, Debug, Default)]
pub(crate) struct ShellReconcileResult {
    pub active: Vec<ActiveSubagentRow>,
    /// Keys that were just auto-removed after successful completion flash.
    pub auto_removed: Vec<String>,
    /// Selection was forced back to parent because selected active finished.
    pub selection_reset_to_parent: bool,
}

/// Pure reconcile: partition live children into active list / history archive.
///
/// Rules:
/// - Working/waiting/blocked/error stay active (unless manually hidden).
/// - Completed: flash briefly, then archive to history and drop from active.
/// - Auto-remove never deletes history entries or parent results.
/// - Archived keys do not reappear in active even if topology still lists them.
pub(crate) fn reconcile_shell_membership(
    state: &mut CodexSessionShellState,
    live: &[LiveChildInput],
    now: Instant,
    flash: Duration,
) -> ShellReconcileResult {
    let mut result = ShellReconcileResult::default();
    let mut selection_reset = false;

    for child in live {
        if state.manually_hidden.contains(&child.child_key)
            || state.archived.contains(&child.child_key)
        {
            continue;
        }

        let status = state
            .status_overrides
            .get(&child.child_key)
            .copied()
            .unwrap_or(child.status);

        if status == AgentTabStatus::Completed {
            let seen = state
                .completion_seen_at
                .entry(child.child_key.clone())
                .or_insert(now);
            let elapsed = now.saturating_duration_since(*seen);
            if elapsed < flash {
                result.active.push(ActiveSubagentRow {
                    child_key: child.child_key.clone(),
                    display_name: child.display_name.clone(),
                    status,
                    status_label: status_label_es(status).into(),
                    task_summary: child.task_summary.clone(),
                    activity: child.activity.clone(),
                    needs_attention: false,
                    completion_flash: true,
                    elapsed_label: child.elapsed_label.clone(),
                });
            } else {
                // Archive only when the record is rich enough; otherwise keep visible.
                match try_archive_completed(state, child, status) {
                    ArchiveOutcome::Archived => {
                        result.auto_removed.push(child.child_key.clone());
                        if matches!(
                            &state.selection,
                            ShellSelection::Active(k) if k == &child.child_key
                        ) {
                            state.selection = ShellSelection::Parent;
                            selection_reset = true;
                        }
                    }
                    ArchiveOutcome::Pending => {
                        state.pending_archive.insert(child.child_key.clone());
                        result.active.push(ActiveSubagentRow {
                            child_key: child.child_key.clone(),
                            display_name: child.display_name.clone(),
                            status: AgentTabStatus::Completed,
                            status_label: "Finalizado · pendiente de archivar".into(),
                            task_summary: child.task_summary.clone(),
                            activity: child.activity.clone(),
                            needs_attention: true,
                            completion_flash: false,
                            elapsed_label: child.elapsed_label.clone(),
                        });
                    }
                }
            }
            continue;
        }

        // Non-completed: clear stale completion timer if status re-opened.
        state.completion_seen_at.remove(&child.child_key);
        state.pending_archive.remove(&child.child_key);

        if status_needs_active_slot(status) {
            result.active.push(ActiveSubagentRow {
                child_key: child.child_key.clone(),
                display_name: child.display_name.clone(),
                status,
                status_label: status_label_es(status).into(),
                task_summary: child.task_summary.clone(),
                activity: child.activity.clone(),
                needs_attention: matches!(
                    status,
                    AgentTabStatus::Blocked | AgentTabStatus::Failed | AgentTabStatus::Waiting
                ),
                completion_flash: false,
                elapsed_label: child.elapsed_label.clone(),
            });
        }
    }

    // Retry pending archives when richer live data or disk recovers.
    let retried = state.retry_pending_archives(live);
    for key in &retried {
        result.auto_removed.push(key.clone());
        result.active.retain(|a| &a.child_key != key);
        if matches!(&state.selection, ShellSelection::Active(k) if k == key) {
            state.selection = ShellSelection::Parent;
            selection_reset = true;
        }
    }

    // If selection points at a missing active row, fall back to parent.
    if let ShellSelection::Active(key) = &state.selection {
        if !result.active.iter().any(|r| &r.child_key == key) {
            state.selection = ShellSelection::Parent;
            selection_reset = true;
        }
    }

    result.selection_reset_to_parent = selection_reset;
    result
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArchiveOutcome {
    Archived,
    /// Validation failed — keep in active list until enough data exists.
    Pending,
}

/// Build a history entry from live + optional rich detail fields.
///
/// Does **not** invent hollow “Completado: {objective}” / “Trabajo sobre …”
/// templates. Work and result must come from real evidence (summaries, tools,
/// activity highlights) or an explicit no-result explanation when work exists.
pub(crate) fn build_history_entry_from_live(
    child: &LiveChildInput,
    status: AgentTabStatus,
) -> HistorySubagentEntry {
    let finished = now_ms();
    let objective = child
        .objective
        .clone()
        .or_else(|| child.task_summary.clone())
        .filter(|s| !looks_like_raw_noise(s));

    let real_highlights: Vec<String> = child
        .activity_highlights
        .iter()
        .filter(|l| !looks_like_raw_noise(l))
        .cloned()
        .collect();
    let has_tool_evidence = !child.tools_used.is_empty();
    let has_activity_evidence = !real_highlights.is_empty();

    // Work: only real summary or derived from non-noise activity/tools (not objective alone).
    let work = child
        .work_summary
        .clone()
        .filter(|s| !looks_like_raw_noise(s))
        .or_else(|| {
            if has_activity_evidence {
                Some(
                    real_highlights
                        .iter()
                        .take(6)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" · "),
                )
            } else if has_tool_evidence {
                Some(format!(
                    "Acciones ejecutadas: {}",
                    child
                        .tools_used
                        .iter()
                        .take(8)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            } else {
                None
            }
        });

    let has_real_work = work.is_some();
    // Result: real handoff only, or explicit no-result *when work evidence exists*.
    let result = child
        .result_summary
        .clone()
        .filter(|s| !looks_like_raw_noise(s))
        .or_else(|| {
            if has_real_work {
                Some("Sin resultado entregado al agente padre.".into())
            } else {
                None
            }
        });

    let files = if child.files_changed.is_empty() {
        // Only attach the explicit no-file note when we otherwise have a record.
        if has_real_work || result.is_some() {
            vec!["No se realizaron cambios en archivos.".into()]
        } else {
            Vec::new()
        }
    } else {
        child.files_changed.clone()
    };

    HistorySubagentEntry {
        id: format!("{}-{}", child.child_key, finished),
        child_key: child.child_key.clone(),
        display_name: child.display_name.clone(),
        parent_label: "Agente principal".into(),
        platform: "Codex".into(),
        status: HistoryStatus::from_tab(status),
        status_label: status_label_es(status).into(),
        result_summary: result.clone(),
        task_summary: child.task_summary.clone(),
        objective,
        work_summary: work,
        activity_highlights: real_highlights,
        tools_used: child.tools_used.clone(),
        files_changed: files,
        parent_handoff: result,
        errors: child.errors.clone(),
        pending: child.pending.clone(),
        started_at_ms: child.started_at_ms.unwrap_or(finished.saturating_sub(1)),
        finished_at_ms: finished,
        technical_details: child.technical_details.clone(),
    }
}

/// Minimum viable archive (acceptance): objective, work summary, final status,
/// start/end, result-or-explanation, handoff-to-parent, files-or-no-file-changes.
///
/// Rejects name/status/date-only and raw-log-only payloads.
pub(crate) fn history_entry_is_archivable(entry: &HistorySubagentEntry) -> bool {
    let objective = entry
        .objective
        .as_ref()
        .or(entry.task_summary.as_ref())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let Some(objective) = objective else {
        return false;
    };
    // Raw internals are not a valid objective.
    if looks_like_raw_noise(objective) {
        return false;
    }

    let work = entry
        .work_summary
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let work_ok = match work {
        Some(w) if !looks_like_raw_noise(w) => true,
        // Allow structured activity/tools as work evidence when summary is thin.
        _ => {
            entry
                .activity_highlights
                .iter()
                .any(|l| !looks_like_raw_noise(l))
                || !entry.tools_used.is_empty()
        }
    };
    if !work_ok {
        return false;
    }

    let result = entry
        .result_summary
        .as_ref()
        .or(entry.parent_handoff.as_ref())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let Some(result) = result else {
        return false;
    };
    if looks_like_raw_noise(result) {
        return false;
    }

    let has_times = entry.finished_at_ms > 0 && entry.started_at_ms > 0;
    let has_files_note = !entry.files_changed.is_empty();
    let has_status = !entry.status_label.trim().is_empty();
    has_times && has_files_note && has_status
}

/// System/permission/JSON noise must not count as human objective/work/result.
pub(crate) fn looks_like_raw_noise(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return true;
    }
    let lower = t.to_lowercase();
    lower.starts_with('{')
        || lower.starts_with('[')
        || lower.contains("\"type\":")
        || lower.contains("message type:")
        || lower.contains("new_task")
        || lower.contains("system prompt")
        || lower.contains("agents.md")
        || lower.contains("permission")
            && (lower.contains("allow") || lower.contains("deny") || lower.contains("policy"))
        || lower.contains("sandbox_permissions")
}

fn try_archive_completed(
    state: &mut CodexSessionShellState,
    child: &LiveChildInput,
    status: AgentTabStatus,
) -> ArchiveOutcome {
    if state.archived.contains(&child.child_key) {
        return ArchiveOutcome::Archived;
    }
    let entry = build_history_entry_from_live(child, status);
    if !history_entry_is_archivable(&entry) {
        // Thin / hollow records must not enter history.
        return ArchiveOutcome::Pending;
    }

    // Verify disk persist **before** dropping from active / marking archived.
    let mut trial = state.history.clone();
    if !trial.iter().any(|h| h.child_key == child.child_key) {
        trial.push(entry.clone());
    }
    let path = state
        .history_store_path_override
        .clone()
        .unwrap_or_else(history_store_path);
    let key = match state.persistence_key.as_deref() {
        Some(k) => k,
        None => {
            // Cannot safely persist without a session key.
            return ArchiveOutcome::Pending;
        }
    };
    if let Err(_err) = save_history_store_to(&path, key, &trial) {
        return ArchiveOutcome::Pending;
    }

    // Disk write succeeded — commit in-memory transition.
    state.history = trial;
    state.archived.insert(child.child_key.clone());
    state.pending_archive.remove(&child.child_key);
    state.completion_seen_at.remove(&child.child_key);
    ArchiveOutcome::Archived
}

/// History list filters / sort (pure, testable).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum HistorySort {
    #[default]
    FinishedNewest,
    FinishedOldest,
    DurationLongest,
    DurationShortest,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HistoryListFilter {
    pub query: String,
    /// When true, only entries with errors.
    pub errors_only: bool,
    /// When true, only entries that changed project files (not the "no changes" note).
    pub files_changed_only: bool,
    /// Optional platform/model substring (e.g. "Codex").
    pub platform: Option<String>,
    /// Optional parent label substring.
    pub parent: Option<String>,
    /// When set, only entries finished at or after this ms (e.g. start of today).
    pub finished_since_ms: Option<u64>,
    pub sort: HistorySort,
}

/// Filter + sort history for the nav list.
pub(crate) fn filter_history<'a>(
    history: &'a [HistorySubagentEntry],
    query: &str,
) -> Vec<&'a HistorySubagentEntry> {
    filter_history_with(
        history,
        &HistoryListFilter {
            query: query.to_string(),
            ..Default::default()
        },
    )
}

pub(crate) fn filter_history_with<'a>(
    history: &'a [HistorySubagentEntry],
    filter: &HistoryListFilter,
) -> Vec<&'a HistorySubagentEntry> {
    let q = filter.query.trim().to_lowercase();
    let mut out: Vec<&HistorySubagentEntry> = history
        .iter()
        .filter(|h| {
            if filter.errors_only && h.errors.is_empty() && h.status != HistoryStatus::Failed {
                return false;
            }
            if filter.files_changed_only {
                let real_files = h.files_changed.iter().any(|f| {
                    !f.to_lowercase().contains("no se realizaron") && !f.trim().is_empty()
                });
                if !real_files {
                    return false;
                }
            }
            if let Some(platform) = &filter.platform {
                if !h
                    .platform
                    .to_lowercase()
                    .contains(&platform.to_lowercase())
                {
                    return false;
                }
            }
            if let Some(parent) = &filter.parent {
                if !h
                    .parent_label
                    .to_lowercase()
                    .contains(&parent.to_lowercase())
                {
                    return false;
                }
            }
            if let Some(since) = filter.finished_since_ms {
                if h.finished_at_ms < since {
                    return false;
                }
            }
            if q.is_empty() {
                return true;
            }
            h.display_name.to_lowercase().contains(&q)
                || h.task_summary
                    .as_ref()
                    .is_some_and(|t| t.to_lowercase().contains(&q))
                || h.objective
                    .as_ref()
                    .is_some_and(|t| t.to_lowercase().contains(&q))
                || h.work_summary
                    .as_ref()
                    .is_some_and(|t| t.to_lowercase().contains(&q))
                || h.result_summary
                    .as_ref()
                    .is_some_and(|t| t.to_lowercase().contains(&q))
                || h.tools_used.iter().any(|t| t.to_lowercase().contains(&q))
        })
        .collect();

    out.sort_by(|a, b| match filter.sort {
        HistorySort::FinishedNewest => b.finished_at_ms.cmp(&a.finished_at_ms),
        HistorySort::FinishedOldest => a.finished_at_ms.cmp(&b.finished_at_ms),
        HistorySort::DurationLongest => {
            let da = a.finished_at_ms.saturating_sub(a.started_at_ms);
            let db = b.finished_at_ms.saturating_sub(b.started_at_ms);
            db.cmp(&da)
        }
        HistorySort::DurationShortest => {
            let da = a.finished_at_ms.saturating_sub(a.started_at_ms);
            let db = b.finished_at_ms.saturating_sub(b.started_at_ms);
            da.cmp(&db)
        }
    });
    out
}

/// Human-readable clipboard summary for a history entry.
pub(crate) fn format_history_copy_summary(entry: &HistorySubagentEntry) -> String {
    let mut parts = vec![
        format!("# {}", entry.display_name),
        format!("Estado: {}", entry.status_label),
        format!(
            "Plataforma: {} · Padre: {}",
            entry.platform, entry.parent_label
        ),
        format!(
            "Duración: {}",
            format_duration_ms(entry.started_at_ms, entry.finished_at_ms)
        ),
    ];
    if let Some(o) = entry.objective.as_ref().or(entry.task_summary.as_ref()) {
        parts.push(format!("Objetivo: {o}"));
    }
    if let Some(w) = &entry.work_summary {
        parts.push(format!("Trabajo: {w}"));
    }
    if let Some(r) = entry.result_summary.as_ref().or(entry.parent_handoff.as_ref()) {
        parts.push(format!("Resultado: {r}"));
    }
    if !entry.files_changed.is_empty() {
        parts.push(format!("Archivos: {}", entry.files_changed.join(", ")));
    }
    if !entry.errors.is_empty() {
        parts.push(format!("Errores: {}", entry.errors.join("; ")));
    }
    if !entry.pending.is_empty() {
        parts.push(format!("Pendientes: {}", entry.pending.join("; ")));
    }
    parts.join("\n")
}

pub(crate) fn format_history_copy_result(entry: &HistorySubagentEntry) -> String {
    entry
        .result_summary
        .clone()
        .or_else(|| entry.parent_handoff.clone())
        .unwrap_or_else(|| "Sin resultado guardado.".into())
}

/// Prompt re-injected into the parent agent to re-run a similar task.
pub(crate) fn format_history_rerun_prompt(entry: &HistorySubagentEntry) -> Option<String> {
    let objective = entry
        .objective
        .as_ref()
        .or(entry.task_summary.as_ref())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())?;
    let mut prompt = format!(
        "Re-run a similar task to the previous subagent \"{}\".\n\nObjective:\n{objective}\n",
        entry.display_name
    );
    if let Some(work) = entry.work_summary.as_ref().filter(|s| !s.trim().is_empty()) {
        prompt.push_str("\nPrevious work notes:\n");
        prompt.push_str(work.trim());
        prompt.push('\n');
    }
    if !entry.files_changed.is_empty() {
        let files: Vec<_> = entry
            .files_changed
            .iter()
            .filter(|f| !f.to_lowercase().contains("no se realizaron"))
            .take(12)
            .cloned()
            .collect();
        if !files.is_empty() {
            prompt.push_str("\nFiles previously touched:\n");
            for f in files {
                prompt.push_str("- ");
                prompt.push_str(&f);
                prompt.push('\n');
            }
        }
    }
    prompt.push_str("\nPlease continue or redo this work with the current repo state.\n");
    Some(prompt)
}

/// Group consecutive same-kind tool lines for compact timeline display.
/// Returns (title, children) groups; non-groupable lines are single-item groups.
pub(crate) fn group_timeline_actions(lines: &[String]) -> Vec<(String, Vec<String>)> {
    let mut groups: Vec<(Option<&'static str>, String, Vec<String>)> = Vec::new();
    for line in lines {
        let kind = timeline_action_kind(line);
        if let Some((prev_kind, title, kids)) = groups.last_mut() {
            if kind.is_some() && *prev_kind == kind {
                kids.push(line.clone());
                *title = format!(
                    "{} · {} acciones",
                    kind.unwrap_or("Acción"),
                    kids.len()
                );
                continue;
            }
        }
        let title = if let Some(k) = kind {
            format!("{k} · 1 acción")
        } else {
            line.clone()
        };
        groups.push((kind, title, vec![line.clone()]));
    }
    groups
        .into_iter()
        .map(|(_, title, kids)| (title, kids))
        .collect()
}

fn timeline_action_kind(line: &str) -> Option<&'static str> {
    let l = line.to_lowercase();
    if l.contains("search")
        || l.contains("búsqueda")
        || l.contains("busqueda")
        || line.contains('🔍')
        || l.contains("web_search")
    {
        Some("Búsqueda web")
    } else if line.contains('🛠')
        || l.contains("function_call")
        || l.starts_with("tool")
        || l.contains("herramienta")
    {
        Some("Herramienta")
    } else if line.contains('💻') || l.contains("shell") || l.contains("exec") {
        Some("Shell")
    } else if line.contains('📄') || (l.contains("file") && !l.contains("profile")) {
        Some("Archivo")
    } else {
        None
    }
}

/// Compact duration between start and end ms.
pub(crate) fn format_duration_ms(started_at_ms: u64, finished_at_ms: u64) -> String {
    if finished_at_ms == 0 || started_at_ms == 0 || finished_at_ms < started_at_ms {
        return "—".into();
    }
    let secs = (finished_at_ms - started_at_ms) / 1000;
    if secs < 60 {
        format!("{secs} s")
    } else if secs < 3600 {
        format!("{} min {} s", secs / 60, secs % 60)
    } else {
        format!("{} h {} min", secs / 3600, (secs % 3600) / 60)
    }
}

/// One-line history card subtitle.
pub(crate) fn history_card_subtitle(entry: &HistorySubagentEntry) -> String {
    let summary = entry
        .work_summary
        .as_ref()
        .or(entry.objective.as_ref())
        .or(entry.task_summary.as_ref())
        .or(entry.result_summary.as_ref())
        .map(|s| truncate_str(s, 72))
        .unwrap_or_else(|| "Ejecución archivada".into());
    summary
}

pub(crate) fn history_card_meta(entry: &HistorySubagentEntry) -> String {
    let dur = format_duration_ms(entry.started_at_ms, entry.finished_at_ms);
    let when = format_finished_at_ms(entry.finished_at_ms);
    let actions = entry.tools_used.len().max(entry.activity_highlights.len());
    let files = if entry.files_changed.iter().any(|f| f.contains("No se realizaron")) {
        "Sin cambios en archivos".to_string()
    } else {
        format!("{} archivos", entry.files_changed.len())
    };
    format!(
        "{} · {dur} · {when} · {actions} acciones · {files}",
        entry.platform
    )
}

fn truncate_str(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

/// Disk store: map session_key → history entries.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct HistoryDiskStore {
    #[serde(default)]
    sessions: HashMap<String, Vec<HistorySubagentEntry>>,
}

pub(crate) fn history_store_path() -> std::path::PathBuf {
    warp_core::paths::config_local_dir().join("codex-shell-history.json")
}

fn load_history_store(session_key: &str) -> Vec<HistorySubagentEntry> {
    load_history_store_from(&history_store_path(), session_key)
}

fn save_history_store(session_key: &str, history: &[HistorySubagentEntry]) -> std::io::Result<()> {
    save_history_store_to(&history_store_path(), session_key, history)
}

/// Load history for tests and production (path injectable).
pub(crate) fn load_history_store_from(
    path: &std::path::Path,
    session_key: &str,
) -> Vec<HistorySubagentEntry> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(store) = serde_json::from_str::<HistoryDiskStore>(&raw) else {
        return Vec::new();
    };
    store.sessions.get(session_key).cloned().unwrap_or_default()
}

/// Save history for tests and production (path injectable).
pub(crate) fn save_history_store_to(
    path: &std::path::Path,
    session_key: &str,
    history: &[HistorySubagentEntry],
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut store = std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<HistoryDiskStore>(&raw).ok())
        .unwrap_or_default();
    store
        .sessions
        .insert(session_key.to_string(), history.to_vec());
    let data = serde_json::to_vec_pretty(&store).unwrap_or_default();
    std::fs::write(path, data)
}

/// Multi-delete selected history ids (must already be confirmed by caller).
pub(crate) fn delete_history_ids(
    state: &mut CodexSessionShellState,
    ids: &[String],
) -> usize {
    if ids.is_empty() {
        return 0;
    }
    let id_set: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let selected_was_deleted = matches!(
        &state.selection,
        ShellSelection::History(k) if id_set.contains(k.as_str())
            || state.history.iter().any(|h| id_set.contains(h.id.as_str())
                && (h.child_key == *k || h.id == *k))
    );
    let before = state.history.len();
    state.history.retain(|h| !id_set.contains(h.id.as_str()) && !id_set.contains(h.child_key.as_str()));
    let removed = before.saturating_sub(state.history.len());
    if selected_was_deleted {
        state.selection = ShellSelection::Parent;
        state.detail_visible = true;
    }
    if removed > 0 {
        let _ = state.persist_history_now();
    }
    removed
}

/// Pure nav model for the three sections (testable without WarpUI).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShellNavModel {
    pub principal_label: String,
    pub principal_platform: String,
    pub active_count: usize,
    pub active_keys: Vec<String>,
    pub active_status_labels: Vec<String>,
    pub history_count: usize,
    pub history_ids: Vec<String>,
    pub selection: ShellSelection,
}

pub(crate) fn build_nav_model(
    active: &[ActiveSubagentRow],
    history: &[HistorySubagentEntry],
    selection: &ShellSelection,
) -> ShellNavModel {
    ShellNavModel {
        principal_label: "Agente principal".into(),
        principal_platform: "Codex".into(),
        active_count: active.len(),
        active_keys: active.iter().map(|a| a.child_key.clone()).collect(),
        active_status_labels: active.iter().map(|a| a.status_label.clone()).collect(),
        history_count: history.len(),
        history_ids: history.iter().map(|h| h.id.clone()).collect(),
        selection: selection.clone(),
    }
}

/// Split feed lines into primary timeline vs technical noise bucket.
pub(crate) fn partition_activity_feed(lines: &[String]) -> (Vec<String>, Vec<String>) {
    let mut primary = Vec::new();
    let mut technical = Vec::new();
    for line in lines {
        if looks_like_raw_noise(line)
            || line.contains("sandbox_permissions")
            || line.starts_with("function_call")
            || (line.starts_with('{') && line.contains("\"arguments\""))
        {
            technical.push(line.clone());
        } else {
            primary.push(line.clone());
        }
    }
    (primary, technical)
}

/// Collect external children under a parent terminal from projection nodes.
///
/// Includes both `ExternalChild` (subagents) and `ExternalGoal` (structured goals)
/// so col1/col2 selection keys stay consistent. `result_summary` is filled by
/// [`enrich_live_children_with_results`] from the readable detail path.
pub(crate) fn live_children_for_terminal(
    nodes: &[AgentTabNode],
    terminal_view_id: warpui::EntityId,
) -> Vec<LiveChildInput> {
    nodes
        .iter()
        .filter_map(|node| live_child_from_node(node, terminal_view_id, None))
        .collect()
}

/// Build one live row from a projection node when it belongs to `terminal_view_id`.
pub(crate) fn live_child_from_node(
    node: &AgentTabNode,
    terminal_view_id: warpui::EntityId,
    result_summary: Option<String>,
) -> Option<LiveChildInput> {
    let child_key = match &node.id {
        MonitorNodeId::ExternalChild { parent: p, child_key } if *p == terminal_view_id => {
            child_key.clone()
        }
        MonitorNodeId::ExternalGoal { parent: p, goal_key } if *p == terminal_view_id => {
            goal_key.clone()
        }
        _ => return None,
    };
    Some(LiveChildInput {
        child_key,
        display_name: node.display_label.clone(),
        status: node.status,
        task_summary: node.task_summary.clone(),
        activity: node.activity.clone(),
        result_summary,
        objective: node.task_summary.clone(),
        work_summary: None,
        activity_highlights: Vec::new(),
        tools_used: Vec::new(),
        files_changed: Vec::new(),
        errors: Vec::new(),
        pending: Vec::new(),
        technical_details: Vec::new(),
        started_at_ms: node.last_event_ms,
        elapsed_label: node.last_event_ms.map(format_relative_started),
    })
}

fn format_relative_started(event_ms: u64) -> String {
    let now = now_ms();
    if event_ms == 0 || now < event_ms {
        return "recién".into();
    }
    let secs = (now - event_ms) / 1000;
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

/// Match a selection key to either an ExternalChild or ExternalGoal under parent.
pub(crate) fn find_node_for_shell_selection<'a>(
    nodes: &'a [AgentTabNode],
    terminal_view_id: warpui::EntityId,
    key: &str,
) -> Option<&'a AgentTabNode> {
    nodes.iter().find(|n| match &n.id {
        MonitorNodeId::ExternalChild { parent, child_key }
            if *parent == terminal_view_id && child_key == key =>
        {
            true
        }
        MonitorNodeId::ExternalGoal { parent, goal_key }
            if *parent == terminal_view_id && goal_key == key =>
        {
            true
        }
        _ => false,
    })
}

/// Format a history finish timestamp for list + detail (local wall clock).
pub(crate) fn format_finished_at_ms(finished_at_ms: u64) -> String {
    if finished_at_ms == 0 {
        return "sin fecha".into();
    }
    // Prefer human-readable local time when chrono is available via system time delta.
    let now = now_ms();
    if now >= finished_at_ms {
        let secs = (now - finished_at_ms) / 1000;
        if secs < 60 {
            return format!("finalizó hace {secs}s");
        }
        if secs < 3600 {
            return format!("finalizó hace {}m", secs / 60);
        }
        if secs < 86_400 {
            return format!("finalizó hace {}h", secs / 3600);
        }
        return format!("finalizó hace {}d", secs / 86_400);
    }
    format!("finalizó · {finished_at_ms}")
}

/// Snapshot of detail fields used by col2 / selection tests (pure mapping).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShellDetailFields {
    pub name: String,
    pub status_label: String,
    pub parent_label: String,
    pub goal_or_task: Option<String>,
    pub activity: Option<String>,
    pub tools: Vec<String>,
    pub result_summary: Option<String>,
    pub transcript_preview: Vec<String>,
}

/// Map a loaded `SubagentDetail` into the fields col2 must show (no UI).
pub(crate) fn shell_detail_fields_from_detail(
    detail: &crate::workspace::agent_monitor_ui::SubagentDetail,
) -> ShellDetailFields {
    ShellDetailFields {
        name: detail.display_name.clone(),
        status_label: detail.status_label.clone(),
        parent_label: detail.parent_label.clone(),
        goal_or_task: detail.task_summary.clone(),
        activity: detail.activity.clone(),
        tools: detail.tools.clone(),
        result_summary: detail.result_summary.clone(),
        transcript_preview: detail.transcript_lines.iter().rev().take(12).rev().cloned().collect(),
    }
}

/// Whether the shell chrome should render.
/// Active subagents keep the shell open; finished-only sessions keep a compact
/// nav so "Ver historial" stays reachable without re-filling active tabs.
pub(crate) fn should_show_shell(active_count: usize, _history_open: bool, history_len: usize) -> bool {
    active_count > 0 || history_len > 0
}

/// Activity indicator glyph for compact nav rows.
pub(crate) fn activity_indicator(status: AgentTabStatus, completion_flash: bool) -> &'static str {
    if completion_flash {
        return "✓";
    }
    match status {
        AgentTabStatus::Working => "●",
        AgentTabStatus::Waiting => "○",
        AgentTabStatus::Blocked => "!",
        AgentTabStatus::Failed => "✕",
        AgentTabStatus::Completed => "✓",
        AgentTabStatus::Unavailable => "·",
    }
}

#[cfg(test)]
#[path = "codex_session_shell_tests.rs"]
mod tests;
