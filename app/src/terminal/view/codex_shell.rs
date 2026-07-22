//! Codex session shell: main content in the center, agents list as the right column.
//!
//! Layout (relative to the workspace rail on the left):
//!   [vertical tabs] | [center: terminal or full subagent activity] | [agents nav]

use std::cell::RefCell;
use std::time::{Duration, Instant};

use pathfinder_color::ColorU;
use warp_core::ui::icons::Icon as CoreIcon;
use warp_core::ui::theme::color::internal_colors;
use warpui::elements::{
    Border, ClippedScrollable, ConstrainedBox, Container, CornerRadius, CrossAxisAlignment,
    Element, Empty, Expanded, Fill as ElementFill, Flex, Hoverable, MainAxisAlignment,
    MainAxisSize, MouseStateHandle, Padding, ParentElement, Radius, Rect, ScrollbarWidth,
    Shrinkable, Text,
};
use warpui::fonts::Weight;
use warpui::platform::Cursor;
use warpui::text_layout::ClipConfig;
use warpui::ui_components::components::{UiComponent, UiComponentStyles};
use warpui::{AppContext, EntityId, SingletonEntity};

use super::TerminalView;
use crate::ai::agent_management::profiles::{
    AgentProfile, AgentProfileStore, AvatarKind, Palette,
};
use crate::appearance::Appearance;
use crate::terminal::cli_agent_sessions::CLIAgentSessionsModel;
use crate::ui_components::avatar::{Avatar, AvatarContent};
use crate::workspace::WorkspaceAction;
use crate::workspace::agent_monitor_ui::{SubagentDetail, load_subagent_detail};
use crate::workspace::agent_presentation::AgentUiProfile;
use crate::workspace::agent_tabs_projection::{AgentTabStatus, AgentTabsProjection, ExternalProvider};
use crate::workspace::codex_session_shell::{
    COMPLETION_FLASH_MS, CodexSessionShellState, HistorySubagentEntry, LiveChildInput,
    ShellSelection, activity_indicator, find_node_for_shell_selection, format_finished_at_ms,
    live_children_for_terminal, reconcile_shell_membership, should_show_shell,
};

const NAV_COL_WIDTH: f32 = 268.;
/// How many activity feed lines to show in the center column.
const DETAIL_FEED_LINES: usize = 80;
const DETAIL_TOOL_LINES: usize = 40;
const CARD_RADIUS: f32 = 6.;
const PILL_RADIUS: f32 = 999.;
const ACCENT_BAR_W: f32 = 2.;
const SHELL_AVATAR_SIZE: f32 = 26.;
const STATUS_DOT: f32 = 8.;
const NAV_GAP: f32 = 6.;

/// Enrich live rows from structured rollouts (tools, files, work, result).
fn enrich_live_children_with_results(
    projection: &AgentTabsProjection,
    terminal_view_id: EntityId,
    live: &mut [LiveChildInput],
) {
    for child in live.iter_mut() {
        let Some(node) =
            find_node_for_shell_selection(&projection.nodes, terminal_view_id, &child.child_key)
        else {
            continue;
        };
        let detail = load_subagent_detail(projection, node);
        if child.result_summary.is_none() {
            if let Some(result) = detail.result_summary.clone() {
                child.result_summary = Some(result);
            } else if matches!(child.status, AgentTabStatus::Completed) {
                child.result_summary = detail
                    .task_summary
                    .clone()
                    .or_else(|| child.task_summary.clone());
            }
        }
        if child.objective.is_none() {
            child.objective = detail.task_summary.clone().or(child.task_summary.clone());
        }
        if child.work_summary.is_none() && !detail.transcript_lines.is_empty() {
            // Prefer human-readable agent messages over tool noise.
            let human: Vec<_> = detail
                .transcript_lines
                .iter()
                .filter(|l| l.starts_with('💬') || l.starts_with('👤') || !l.starts_with('🛠'))
                .take(4)
                .cloned()
                .collect();
            if !human.is_empty() {
                child.work_summary = Some(human.join(" · "));
            }
        }
        if child.tools_used.is_empty() && !detail.tools.is_empty() {
            child.tools_used = detail.tools.clone();
        }
        if child.files_changed.is_empty() && !detail.files.is_empty() {
            child.files_changed = detail.files.clone();
        }
        if child.activity_highlights.is_empty() && !detail.transcript_lines.is_empty() {
            child.activity_highlights = detail
                .transcript_lines
                .iter()
                .rev()
                .take(12)
                .rev()
                .cloned()
                .collect();
        }
        // Technical: keep raw tool lines separately for collapsible section.
        if child.technical_details.is_empty() {
            child.technical_details = detail
                .tools
                .iter()
                .filter(|t| t.len() > 80 || t.contains('{'))
                .cloned()
                .collect();
        }
        if child.elapsed_label.is_none() {
            child.elapsed_label = detail.elapsed_label.clone();
        }
        if child.started_at_ms.is_none() {
            child.started_at_ms = detail.last_event_ms;
        }
        if child.activity.is_none() {
            child.activity = detail.activity.clone();
        }
    }
}

impl TerminalView {
    pub(super) fn codex_shell_state(&self) -> &RefCell<CodexSessionShellState> {
        &self.codex_shell
    }

    pub(crate) fn apply_codex_shell_action(
        &mut self,
        action: CodexShellAction,
        ctx: &mut warpui::ViewContext<Self>,
    ) {
        if matches!(action, CodexShellAction::RetryPendingArchives) {
            let projection = self.session_agent_projection(ctx);
            let mut live = projection
                .as_ref()
                .map(|p| live_children_for_terminal(&p.nodes, self.view_id))
                .unwrap_or_default();
            if let Some(p) = projection.as_ref() {
                enrich_live_children_with_results(p, self.view_id, &mut live);
            }
            let _ = self
                .codex_shell
                .borrow_mut()
                .retry_pending_archives(&live);
            ctx.notify();
            return;
        }

        let mut shell = self.codex_shell.borrow_mut();
        match action {
            CodexShellAction::SelectParent => shell.select_parent(),
            CodexShellAction::SelectSubagent { child_key } => shell.select_active(child_key),
            CodexShellAction::CloseView => shell.close_view(),
            CodexShellAction::ToggleHistory => shell.toggle_history(),
            CodexShellAction::SelectHistory { child_key } => shell.select_history(child_key),
            CodexShellAction::RemoveFromList { child_key } => {
                let is_working = self
                    .live_child_is_working(&child_key, ctx)
                    .unwrap_or(false);
                let _ = shell.request_remove_from_list(&child_key, is_working);
            }
            CodexShellAction::Stop { child_key } => {
                let _ = shell.request_stop(
                    &child_key,
                    CodexSessionShellState::stop_action_available(),
                );
            }
            CodexShellAction::DeleteHistory { history_id } => {
                let _ = shell.request_delete_history(&history_id);
            }
            CodexShellAction::ClearHistory => {
                let _ = shell.request_clear_history();
            }
            CodexShellAction::ToggleTechnicalDetails => shell.toggle_technical_details(),
            CodexShellAction::ToggleHistoryMultiSelect => shell.toggle_history_multi_select(),
            CodexShellAction::ToggleHistoryIdSelected { history_id } => {
                shell.toggle_history_id_selected(&history_id);
            }
            CodexShellAction::DeleteSelectedHistory => {
                let _ = shell.request_delete_selected_history();
            }
            CodexShellAction::CycleHistorySort => shell.cycle_history_sort(),
            CodexShellAction::ToggleHistoryErrorsFilter => shell.toggle_history_errors_only(),
            CodexShellAction::ToggleHistoryFilesFilter => shell.toggle_history_files_only(),
            CodexShellAction::ToggleTimelineGroup { group_key } => {
                shell.toggle_timeline_group(&group_key);
            }
            CodexShellAction::SelectAllVisibleHistory { ids } => {
                shell.select_all_visible_history(&ids);
                shell.history_multi_select = true;
            }
            CodexShellAction::SetHistoryQuery { query } => {
                let clear_editor = query.is_empty();
                shell.set_history_query(query);
                drop(shell);
                if clear_editor {
                    self.codex_history_search.update(ctx, |ed, ctx| {
                        ed.set_buffer_text("", ctx);
                    });
                }
                ctx.notify();
                return;
            }
            CodexShellAction::ToggleHistoryTodayFilter => shell.toggle_history_today_only(),
            CodexShellAction::RetryPendingArchives => {
                // Handled above before borrow.
            }
            CodexShellAction::CancelConfirms => shell.cancel_pending_confirms(),
        }
        drop(shell);
        ctx.notify();
    }

    fn live_child_is_working(&self, child_key: &str, app: &AppContext) -> Option<bool> {
        let projection = self.session_agent_projection(app)?;
        let live = live_children_for_terminal(&projection.nodes, self.view_id);
        live.into_iter()
            .find(|c| c.child_key == child_key)
            .map(|c| {
                matches!(
                    c.status,
                    crate::workspace::agent_tabs_projection::AgentTabStatus::Working
                        | crate::workspace::agent_tabs_projection::AgentTabStatus::Waiting
                        | crate::workspace::agent_tabs_projection::AgentTabStatus::Unavailable
                )
            })
    }

    fn session_agent_projection(&self, app: &AppContext) -> Option<AgentTabsProjection> {
        if CLIAgentSessionsModel::as_ref(app)
            .session(self.view_id)
            .is_none()
        {
            return None;
        }
        let external_sessions = AgentTabsProjection::external_sessions_from_model(
            CLIAgentSessionsModel::as_ref(app).sessions_snapshot(),
        );
        Some(AgentTabsProjection::from_snapshots(
            std::iter::empty(),
            external_sessions,
        ))
    }

    /// Wrap the terminal in a 3-column-aware shell when this pane has subagents:
    /// center = full terminal or full subagent activity; right = agents list.
    pub(super) fn maybe_wrap_codex_session_shell(
        &self,
        content: Box<dyn Element>,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let Some(projection) = self.session_agent_projection(app) else {
            return content;
        };
        let mut live = live_children_for_terminal(&projection.nodes, self.view_id);
        enrich_live_children_with_results(&projection, self.view_id, &mut live);
        if live.is_empty() && self.codex_shell.borrow().history.is_empty() {
            return content;
        }

        let profile = session_ui_profile(app, self.view_id);
        // Leaf-only providers (Grok, …) never get deep-dive chrome — fleet is enough.
        if !profile.capabilities.session_shell
            && self.codex_shell.borrow().history.is_empty()
        {
            return content;
        }

        let mut shell = self.codex_shell.borrow_mut();
        // Persist history across reloads using CLI session id when available.
        if let Some(sess) = CLIAgentSessionsModel::as_ref(app).session(self.view_id) {
            let key = sess
                .session_context
                .session_id
                .clone()
                .unwrap_or_else(|| format!("pane-{}", self.view_id));
            shell.load_persisted_history(&key);
        }
        let reconcile = reconcile_shell_membership(
            &mut shell,
            &live,
            Instant::now(),
            Duration::from_millis(COMPLETION_FLASH_MS),
        );
        let active = reconcile.active;
        let history_open = shell.history_open;
        let history_len = shell.history.len();
        let show = should_show_shell(active.len(), history_open, history_len);
        if !show {
            return content;
        }

        let selection = shell.selection.clone();
        let detail_visible = shell.detail_visible;
        let history_entries = shell.history.clone();
        let confirm_remove = shell.confirm_remove.clone();
        let confirm_stop = shell.confirm_stop.clone();
        let show_technical = shell.show_technical_details;
        let terminal_view_id = self.view_id;
        let detail_scroll = shell.detail_scroll.clone();

        let history_search = self.codex_history_search.clone();
        let nav = render_codex_nav_column(
            &mut shell,
            &active,
            &live,
            &selection,
            history_open,
            &history_entries,
            terminal_view_id,
            profile,
            &history_search,
            app,
        );
        drop(shell);

        // Center: parent terminal, or full subagent activity feed (not only goal).
        let center: Box<dyn Element> = if detail_visible {
            match &selection {
                ShellSelection::Active(key) => {
                    let detail = find_node_for_shell_selection(
                        &projection.nodes,
                        terminal_view_id,
                        key,
                    )
                    .map(|node| load_subagent_detail(&projection, node))
                    .or_else(|| {
                        active.iter().find(|r| &r.child_key == key).map(|row| {
                            SubagentDetail {
                                child_key: row.child_key.clone(),
                                display_name: row.display_name.clone(),
                                parent_label: profile.product_name.into(),
                                breadcrumb: vec![
                                    profile.product_name.into(),
                                    row.display_name.clone(),
                                ],
                                status: row.status,
                                status_label: row.status_label.clone(),
                                task_summary: row.task_summary.clone(),
                                activity: row.activity.clone(),
                                tools: Vec::new(),
                                files: Vec::new(),
                                transcript_lines: Vec::new(),
                                last_event_ms: None,
                                elapsed_label: None,
                                result_summary: None,
                            }
                        })
                    });
                    if let Some(detail) = detail {
                        render_codex_detail_column(
                            &detail,
                            terminal_view_id,
                            key,
                            confirm_remove.as_deref() == Some(key.as_str()),
                            confirm_stop.as_deref() == Some(key.as_str()),
                            /*is_history=*/ false,
                            /*show_stop=*/ CodexSessionShellState::stop_action_available(),
                            show_technical,
                            profile,
                            detail_scroll.clone(),
                            app,
                        )
                    } else {
                        content
                    }
                }
                ShellSelection::History(key) => {
                    if let Some(entry) = history_entries
                        .iter()
                        .find(|h| &h.child_key == key || &h.id == key)
                    {
                        render_history_detail_column(
                            entry,
                            terminal_view_id,
                            profile,
                            detail_scroll.clone(),
                            app,
                        )
                    } else {
                        content
                    }
                }
                ShellSelection::Parent => content,
            }
        } else {
            content
        };

        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        // Center = todo (terminal / activity). Right = agents (3ª columna del workspace).
        Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(Shrinkable::new(1., center).finish())
            .with_child(
                ConstrainedBox::new(
                    Container::new(nav)
                        .with_background(internal_colors::fg_overlay_1(theme))
                        .with_border(Border::left(1.).with_border_fill(theme.outline()))
                        .with_padding(Padding::uniform(12.))
                        .finish(),
                )
                .with_width(NAV_COL_WIDTH)
                .finish(),
            )
            .finish()
    }
}

#[derive(Clone, Debug)]
pub(crate) enum CodexShellAction {
    SelectParent,
    SelectSubagent { child_key: String },
    CloseView,
    ToggleHistory,
    SelectHistory { child_key: String },
    RemoveFromList { child_key: String },
    Stop { child_key: String },
    DeleteHistory { history_id: String },
    ClearHistory,
    ToggleTechnicalDetails,
    ToggleHistoryMultiSelect,
    ToggleHistoryIdSelected { history_id: String },
    DeleteSelectedHistory,
    CycleHistorySort,
    ToggleHistoryErrorsFilter,
    ToggleHistoryFilesFilter,
    ToggleTimelineGroup { group_key: String },
    SelectAllVisibleHistory { ids: Vec<String> },
    SetHistoryQuery { query: String },
    ToggleHistoryTodayFilter,
    RetryPendingArchives,
    CancelConfirms,
}

fn session_ui_profile(app: &AppContext, terminal_view_id: EntityId) -> AgentUiProfile {
    let provider = CLIAgentSessionsModel::as_ref(app)
        .session(terminal_view_id)
        .and_then(|s| ExternalProvider::from_cli(s.agent))
        .unwrap_or(ExternalProvider::Other);
    AgentUiProfile::for_provider(provider)
}

/// Stable profile identity for the shell parent (matches rail external roots).
fn shell_profile_identity(
    app: &AppContext,
    terminal_view_id: EntityId,
    profile: AgentUiProfile,
) -> (String, String, String) {
    let provider = profile.provider.profile_provider().to_string();
    let agent_key = CLIAgentSessionsModel::as_ref(app)
        .session(terminal_view_id)
        .and_then(|s| s.session_context.session_id.clone())
        .filter(|k| {
            !k.is_empty()
                && k.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        })
        .unwrap_or_else(|| format!("pane-{terminal_view_id}"));
    let fallback = profile.product_name.to_string();
    (provider, agent_key, fallback)
}

/// Unique file paths touched by live + history children (repo work signal).
fn collect_repo_files(
    live: &[LiveChildInput],
    history: &[HistorySubagentEntry],
) -> Vec<String> {
    let mut out = Vec::new();
    for child in live {
        for f in &child.files_changed {
            let t = f.trim();
            if t.is_empty() || t.contains("No se realizaron") {
                continue;
            }
            if !out.iter().any(|e| e == t) {
                out.push(t.to_string());
            }
        }
    }
    for entry in history {
        for f in &entry.files_changed {
            let t = f.trim();
            if t.is_empty() || t.contains("No se realizaron") {
                continue;
            }
            if !out.iter().any(|e| e == t) {
                out.push(t.to_string());
            }
        }
    }
    out
}

fn render_codex_nav_column(
    shell: &mut CodexSessionShellState,
    active: &[crate::workspace::codex_session_shell::ActiveSubagentRow],
    live: &[LiveChildInput],
    selection: &ShellSelection,
    history_open: bool,
    history: &[HistorySubagentEntry],
    terminal_view_id: EntityId,
    profile: AgentUiProfile,
    history_search: &warpui::ViewHandle<crate::editor::EditorView>,
    app: &AppContext,
) -> Box<dyn Element> {
    use crate::workspace::codex_session_shell::{
        HistorySort, filter_history_with, history_card_meta, history_card_subtitle,
    };
    use warpui::ui_components::components::{UiComponent, UiComponentStyles};
    use warpui::ui_components::text_input::TextInput;

    let appearance = Appearance::as_ref(app);
    let theme = appearance.theme();
    let sub = theme.sub_text_color(theme.background());
    let font = appearance.ui_font_family();
    let profiles = AgentProfileStore::load_default();
    let (provider_key, agent_key, fallback_name) =
        shell_profile_identity(app, terminal_view_id, profile);
    let agent_profile = profiles
        .profiles
        .get(&format!("{provider_key}:{agent_key}"))
        .cloned();
    let display_name = agent_profile
        .as_ref()
        .map(|p| p.display_name.clone())
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| fallback_name.clone());
    let repo_files = collect_repo_files(live, history);

// Fixed chrome stays pinned; list body scrolls (history fill was unusable before).
    let mut header = Flex::column()
        .with_main_axis_size(MainAxisSize::Min)
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(NAV_GAP);

    header = header.with_child(section_label(profile.section_agent, font, sub));
    let parent_selected = matches!(selection, ShellSelection::Parent);
    let parent_ms = shell.row_mouse_state("__parent__");
    let edit_ms = shell.row_mouse_state("__edit_avatar__");
    let parent_meta = {
        let mut parts = vec![profile.parent_subtitle(active.len())];
        if !repo_files.is_empty() {
            parts.push(format!(
                "{} archivo{}",
                repo_files.len(),
                if repo_files.len() == 1 { "" } else { "s" }
            ));
        }
        parts.join(" · ")
    };
    header = header.with_child(parent_identity_card(
        parent_ms,
        edit_ms,
        display_name,
        parent_meta,
        agent_profile.as_ref(),
        fallback_name.as_str(),
        parent_selected,
        appearance,
        theme,
        WorkspaceAction::CodexShellSelectParent { terminal_view_id },
        WorkspaceAction::OpenAgentMonitorProfileEditor {
            provider: provider_key,
            agent_key,
            fallback_name: fallback_name.clone(),
        },
    ));

    if !repo_files.is_empty() {
        let basenames: Vec<String> = repo_files
            .iter()
            .take(4)
            .map(|p| {
                p.rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(p.as_str())
                    .to_string()
            })
            .collect();
        let more = if repo_files.len() > 4 {
            format!(" · +{}", repo_files.len() - 4)
        } else {
            String::new()
        };
        header = header.with_child(repo_chip(
            &format!("repo · {}{}", basenames.join(" · "), more),
            font,
            theme,
        ));
    }

    // —— Scrollable body: activos + historial ——
    let mut body = Flex::column()
        .with_main_axis_size(MainAxisSize::Min)
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(NAV_GAP);

    body = body.with_child(section_label(
        &format!("Activos  {}", active.len()),
        font,
        sub,
    ));

    if active.is_empty() {
        body = body.with_child(quiet_hint(profile.empty_active_hint, font, sub));
    }

    if !shell.pending_archive.is_empty() {
        let retry_ms = shell.row_mouse_state("__retry_archive__");
        body = body.with_child(nav_card(
            retry_ms,
            format!("Reintentar archivo · {}", shell.pending_archive.len()),
            "No se pudo guardar en disco".into(),
            false,
            theme.ansi_fg_yellow(),
            appearance,
            theme,
            WorkspaceAction::CodexShellRetryPendingArchives { terminal_view_id },
            None,
            NavCardKind::Warning,
        ));
    }

    for row in active {
        let selected = matches!(selection, ShellSelection::Active(k) if k == &row.child_key);
        let accent = status_color(row.status, row.completion_flash, theme);
        let task = row
            .task_summary
            .as_ref()
            .or(row.activity.as_ref())
            .map(|s| truncate_ui(s, 48))
            .unwrap_or_else(|| {
                AgentUiProfile::status_chip(row.status, row.completion_flash).to_string()
            });
        let elapsed = row.elapsed_label.clone().unwrap_or_default();
        let file_n = live
            .iter()
            .find(|c| c.child_key == row.child_key)
            .map(|c| {
                c.files_changed
                    .iter()
                    .filter(|f| !f.contains("No se realizaron") && !f.trim().is_empty())
                    .count()
            })
            .unwrap_or(0);
        let mut meta = Vec::new();
        if !elapsed.is_empty() {
            meta.push(elapsed);
        }
        if file_n > 0 {
            meta.push(format!(
                "{file_n} archivo{}",
                if file_n == 1 { "" } else { "s" }
            ));
        }
        meta.push(task);
        let subtitle = meta.join(" · ");
        let ms = shell.row_mouse_state(&row.child_key);
        let key = row.child_key.clone();
        body = body.with_child(nav_card(
            ms,
            row.display_name.clone(),
            subtitle,
            selected,
            accent,
            appearance,
            theme,
            WorkspaceAction::CodexShellSelectSubagent {
                terminal_view_id,
                child_key: key,
            },
            None,
            NavCardKind::Agent,
        ));
    }

    body = body.with_child(section_label(
        &format!("Historial  {}", history.len()),
        font,
        sub,
    ));
    let hist_ms = shell.row_mouse_state("__history__");
    body = body.with_child(nav_card(
        hist_ms,
        if history_open {
            "Ocultar terminados".into()
        } else {
            "Ver terminados".into()
        },
        if history.is_empty() {
            "Sin ejecuciones guardadas".into()
        } else {
            format!(
                "{} guardada{}",
                history.len(),
                if history.len() == 1 { "" } else { "s" }
            )
        },
        history_open,
        theme.ansi_fg_blue(),
        appearance,
        theme,
        WorkspaceAction::CodexShellToggleHistory { terminal_view_id },
        None,
        NavCardKind::Neutral,
    ));

    if history_open {
        shell.history_filter.query = shell.history_query.clone();
        let filtered = filter_history_with(history, &shell.history_filter);
        let multi = shell.history_multi_select;

        if !history.is_empty() {
            // Free-text search (real TextInput, not action stubs).
            let search_field = TextInput::new(
                history_search.clone(),
                UiComponentStyles::default()
                    .set_background(ElementFill::None)
                    .set_border_radius(CornerRadius::with_all(Radius::Pixels(0.)))
                    .set_border_width(0.),
            )
            .build()
            .finish();
            let search_bar = Container::new(
                Flex::row()
                    .with_main_axis_size(MainAxisSize::Max)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_spacing(6.)
                    .with_child(
                        Text::new_inline("⌕".to_string(), font, 11.)
                            .with_color(sub.into())
                            .finish(),
                    )
                    .with_child(Shrinkable::new(1., search_field).finish())
                    .finish(),
            )
            .with_padding(
                Padding::uniform(0.)
                    .with_top(6.)
                    .with_bottom(6.)
                    .with_left(8.)
                    .with_right(8.),
            )
            .with_background(internal_colors::fg_overlay_1(theme))
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(CARD_RADIUS)))
            .with_border(Border::all(1.).with_border_fill(theme.outline()))
            .finish();
            body = body.with_child(search_bar);

            let sort_short = match shell.history_filter.sort {
                HistorySort::FinishedNewest => "Recientes",
                HistorySort::FinishedOldest => "Antiguos",
                HistorySort::DurationLongest => "Más largos",
                HistorySort::DurationShortest => "Más cortos",
            };
            let mut pills = Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_spacing(6.)
                .with_child(filter_pill(
                    shell.row_mouse_state("__hist_sort__"),
                    sort_short,
                    false,
                    appearance,
                    theme,
                    WorkspaceAction::CodexShellCycleHistorySort { terminal_view_id },
                ))
                .with_child(filter_pill(
                    shell.row_mouse_state("__hist_err__"),
                    "Errores",
                    shell.history_filter.errors_only,
                    appearance,
                    theme,
                    WorkspaceAction::CodexShellToggleHistoryErrorsFilter { terminal_view_id },
                ))
                .with_child(filter_pill(
                    shell.row_mouse_state("__hist_files__"),
                    "Archivos",
                    shell.history_filter.files_changed_only,
                    appearance,
                    theme,
                    WorkspaceAction::CodexShellToggleHistoryFilesFilter { terminal_view_id },
                ))
                .with_child(filter_pill(
                    shell.row_mouse_state("__hist_today__"),
                    "Hoy",
                    shell.history_filter.finished_since_ms.is_some(),
                    appearance,
                    theme,
                    WorkspaceAction::CodexShellToggleHistoryTodayFilter { terminal_view_id },
                ));
            if !shell.history_query.is_empty() {
                pills = pills.with_child(filter_pill(
                    shell.row_mouse_state("__hist_query__"),
                    "Limpiar",
                    true,
                    appearance,
                    theme,
                    WorkspaceAction::CodexShellSetHistoryQuery {
                        terminal_view_id,
                        query: String::new(),
                    },
                ));
            }
            body = body.with_child(pills.finish());

            let mut tools = Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_spacing(6.)
                .with_child(filter_pill(
                    shell.row_mouse_state("__hist_multi__"),
                    if multi { "Selección ✓" } else { "Seleccionar" },
                    multi,
                    appearance,
                    theme,
                    WorkspaceAction::CodexShellToggleHistoryMultiSelect { terminal_view_id },
                ));
            let visible_ids: Vec<String> = filtered.iter().map(|e| e.id.clone()).collect();
            if multi && !visible_ids.is_empty() {
                tools = tools.with_child(filter_pill(
                    shell.row_mouse_state("__hist_all_vis__"),
                    "Todos",
                    false,
                    appearance,
                    theme,
                    WorkspaceAction::CodexShellSelectAllVisibleHistory {
                        terminal_view_id,
                        ids: visible_ids,
                    },
                ));
            }
            if multi && !shell.history_selected_ids.is_empty() {
                let bulk = if shell.confirm_multi_delete {
                    format!("¿Borrar {}?", shell.history_selected_ids.len())
                } else {
                    format!("Borrar {}", shell.history_selected_ids.len())
                };
                tools = tools.with_child(filter_pill(
                    shell.row_mouse_state("__hist_bulk_del__"),
                    &bulk,
                    shell.confirm_multi_delete,
                    appearance,
                    theme,
                    WorkspaceAction::CodexShellDeleteSelectedHistory { terminal_view_id },
                ));
            }
            body = body.with_child(tools.finish());
        }

        if history.is_empty() {
            body = body.with_child(quiet_hint(profile.empty_history_hint, font, sub));
        } else if filtered.is_empty() {
            body = body.with_child(quiet_hint("Nada coincide con los filtros.", font, sub));
        } else {
            // No artificial take(30) cap — scroll handles long lists.
            for entry in filtered.into_iter().rev() {
                let selected = matches!(
                    selection,
                    ShellSelection::History(k) if k == &entry.child_key || k == &entry.id
                ) || shell.history_selected_ids.contains(&entry.id);
                let ms = shell.row_mouse_state(&format!("hist:{}", entry.id));
                let key = entry.child_key.clone();
                let line1 = history_card_subtitle(entry);
                let line2 = history_card_meta(entry);
                let id_for_row = entry.id.clone();
                let primary_action = if multi {
                    WorkspaceAction::CodexShellToggleHistoryIdSelected {
                        terminal_view_id,
                        history_id: id_for_row.clone(),
                    }
                } else {
                    WorkspaceAction::CodexShellSelectHistory {
                        terminal_view_id,
                        child_key: key,
                    }
                };
                let trailing = if multi {
                    let mark = if shell.history_selected_ids.contains(&entry.id) {
                        "✓"
                    } else {
                        "○"
                    };
                    Some((
                        mark.into(),
                        WorkspaceAction::CodexShellToggleHistoryIdSelected {
                            terminal_view_id,
                            history_id: id_for_row,
                        },
                    ))
                } else {
                    let confirm_del = shell.confirm_delete_history_id.as_deref()
                        == Some(entry.id.as_str())
                        || shell.confirm_delete_history_id.as_deref()
                            == Some(entry.child_key.as_str());
                    if confirm_del {
                        Some((
                            "¿Borrar?".into(),
                            WorkspaceAction::CodexShellDeleteHistory {
                                terminal_view_id,
                                history_id: id_for_row,
                            },
                        ))
                    } else {
                        None
                    }
                };
                let hist_accent = if !entry.errors.is_empty()
                    || matches!(
                        entry.status,
                        crate::workspace::codex_session_shell::HistoryStatus::Failed
                            | crate::workspace::codex_session_shell::HistoryStatus::Blocked
                    )
                {
                    theme.ansi_fg_red()
                } else {
                    theme.ansi_fg_green()
                };
                body = body.with_child(nav_card(
                    ms,
                    entry.display_name.clone(),
                    format!("{line1} · {line2}"),
                    selected,
                    hist_accent,
                    appearance,
                    theme,
                    primary_action,
                    trailing,
                    NavCardKind::History,
                ));
            }
        }

        if !history.is_empty() {
            let clear_label = if shell.confirm_clear_history {
                format!("¿Limpiar {}?", history.len())
            } else {
                "Limpiar historial".into()
            };
            let clear_ms = shell.row_mouse_state("__clear_history__");
            body = body.with_child(nav_card(
                clear_ms,
                clear_label,
                "Solo borra el registro local".into(),
                false,
                theme.ansi_fg_red(),
                appearance,
                theme,
                WorkspaceAction::CodexShellClearHistory { terminal_view_id },
                None,
                NavCardKind::Danger,
            ));
        }
    }

    let scroll = ClippedScrollable::vertical(
        shell.nav_scroll.clone(),
        body.finish(),
        ScrollbarWidth::Custom(3.),
        theme.nonactive_ui_detail().into(),
        theme.active_ui_detail().into(),
        ElementFill::None,
    )
    .with_overlayed_scrollbar()
    .finish();

    Flex::column()
        .with_main_axis_size(MainAxisSize::Max)
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(NAV_GAP)
        .with_child(header.finish())
        .with_child(Shrinkable::new(1., scroll).finish())
        .finish()
}

fn section_label(
    text: &str,
    font: warpui::fonts::FamilyId,
    color: impl Into<pathfinder_color::ColorU> + Copy,
) -> Box<dyn Element> {
    Container::new(
        Text::new_inline(text.to_string(), font, 9.)
            .with_color(color.into())
            .finish(),
    )
    .with_padding(
        Padding::uniform(0.)
            .with_top(8.)
            .with_bottom(1.)
            .with_left(4.)
            .with_right(2.),
    )
    .finish()
}

/// Quiet empty-state line — no heavy card chrome (Herdr keeps empty UI light).
fn quiet_hint(
    text: &str,
    font: warpui::fonts::FamilyId,
    color: impl Into<pathfinder_color::ColorU> + Copy,
) -> Box<dyn Element> {
    Container::new(
        Text::new_inline(text.to_string(), font, 10.)
            .with_color(color.into())
            .finish(),
    )
    .with_padding(
        Padding::uniform(0.)
            .with_top(4.)
            .with_bottom(4.)
            .with_left(4.)
            .with_right(4.),
    )
    .finish()
}

fn muted_hint(
    text: &str,
    font: warpui::fonts::FamilyId,
    color: impl Into<pathfinder_color::ColorU> + Copy,
    theme: &crate::themes::theme::WarpTheme,
) -> Box<dyn Element> {
    Container::new(
        Text::new_inline(text.to_string(), font, 10.)
            .with_color(color.into())
            .finish(),
    )
    .with_padding(Padding::uniform(8.))
    .with_background(internal_colors::fg_overlay_1(theme))
    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(CARD_RADIUS)))
    .finish()
}

/// Compact repo summary chip (metadata token, not a wall of paths).
fn repo_chip(
    text: &str,
    font: warpui::fonts::FamilyId,
    theme: &crate::themes::theme::WarpTheme,
) -> Box<dyn Element> {
    let sub = theme.sub_text_color(theme.background());
    Container::new(
        Text::new_inline(text.to_string(), font, 10.)
            .with_clip(ClipConfig::ellipsis())
            .with_color(sub.into())
            .finish(),
    )
    .with_padding(
        Padding::uniform(0.)
            .with_top(5.)
            .with_bottom(5.)
            .with_left(8.)
            .with_right(8.),
    )
    .with_background(internal_colors::fg_overlay_1(theme))
    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(PILL_RADIUS)))
    .with_border(Border::all(1.).with_border_fill(theme.outline()))
    .finish()
}

fn status_dot(color: pathfinder_color::ColorU) -> Box<dyn Element> {
    ConstrainedBox::new(
        Container::new(Rect::new().with_background(color).finish())
            .with_corner_radius(CornerRadius::with_all(Radius::Percentage(50.)))
            .finish(),
    )
    .with_width(STATUS_DOT)
    .with_height(STATUS_DOT)
    .finish()
}

fn truncate_ui(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

#[derive(Clone, Copy)]
enum NavCardKind {
    Agent,
    History,
    Neutral,
    Warning,
    Danger,
}

/// Parent row: avatar (edit) + name/meta (select). Dense like Herdr sidebar headers.
#[allow(clippy::too_many_arguments)]
fn parent_identity_card(
    select_ms: MouseStateHandle,
    edit_ms: MouseStateHandle,
    title: String,
    subtitle: String,
    agent_profile: Option<&AgentProfile>,
    fallback_name: &str,
    selected: bool,
    appearance: &Appearance,
    theme: &crate::themes::theme::WarpTheme,
    select_action: WorkspaceAction,
    edit_action: WorkspaceAction,
) -> Box<dyn Element> {
    let main = theme.main_text_color(theme.background());
    let sub = theme.sub_text_color(theme.background());
    let font = appearance.ui_font_family();
    let fallback_name = fallback_name.to_string();
    let agent_profile = agent_profile.cloned();

    let profile_for_avatar = agent_profile.clone();
    let fallback_for_avatar = fallback_name.clone();
    let avatar_btn = Hoverable::new(edit_ms, move |mouse| {
        let ring = if mouse.is_hovered() {
            Border::all(1.5).with_border_fill(theme.accent())
        } else {
            Border::all(1.).with_border_fill(theme.outline())
        };
        Container::new(shell_avatar_element(
            profile_for_avatar.as_ref(),
            &fallback_for_avatar,
            appearance,
        ))
        .with_padding(Padding::uniform(1.))
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(SHELL_AVATAR_SIZE)))
        .with_border(ring)
        .finish()
    })
    .with_cursor(Cursor::PointingHand)
    .on_click(move |ctx, _, _| {
        ctx.dispatch_typed_action(edit_action.clone());
    })
    .finish();

    let text_hit = Hoverable::new(select_ms, move |_| {
        Flex::column()
            .with_main_axis_size(MainAxisSize::Min)
            .with_cross_axis_alignment(CrossAxisAlignment::Start)
            .with_spacing(1.)
            .with_child(
                Text::new_inline(title.clone(), font, 12.)
                    .with_clip(ClipConfig::ellipsis())
                    .with_color(main.into())
                    .finish(),
            )
            .with_child(
                Text::new_inline(subtitle.clone(), font, 10.)
                    .with_clip(ClipConfig::ellipsis())
                    .with_color(sub.into())
                    .finish(),
            )
            .finish()
    })
    .with_cursor(Cursor::PointingHand)
    .on_click(move |ctx, _, _| {
        ctx.dispatch_typed_action(select_action.clone());
    })
    .finish();

    let bg = if selected {
        internal_colors::fg_overlay_2(theme)
    } else {
        internal_colors::fg_overlay_1(theme)
    };
    let border = if selected {
        Border::all(1.).with_border_fill(theme.ansi_fg_blue())
    } else {
        Border::all(1.).with_border_fill(theme.outline())
    };
    let row = Flex::row()
        .with_main_axis_size(MainAxisSize::Max)
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_spacing(8.)
        .with_child(avatar_btn)
        .with_child(Expanded::new(1., text_hit).finish())
        .finish();
    Container::new(row)
        .with_padding(
            Padding::uniform(0.)
                .with_top(8.)
                .with_bottom(8.)
                .with_left(8.)
                .with_right(8.),
        )
        .with_background(bg)
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(CARD_RADIUS)))
        .with_border(border)
        .finish()
}

fn shell_avatar_element(
    profile: Option<&AgentProfile>,
    fallback_name: &str,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let (content, background) = match profile {
        Some(profile) => {
            let content = if let Some(path) = profile.avatar_image_path.as_ref() {
                AvatarContent::LocalImage {
                    path: path.clone(),
                    display_name: profile.display_name.clone(),
                }
            } else {
                match profile.icon {
                    AvatarKind::Initial => {
                        AvatarContent::DisplayName(profile.display_name.clone())
                    }
                    AvatarKind::Assistant => AvatarContent::Icon(CoreIcon::AiAssistant),
                    AvatarKind::Code => AvatarContent::Icon(CoreIcon::Code2),
                    AvatarKind::Terminal => AvatarContent::Icon(CoreIcon::Terminal),
                }
            };
            let background = match profile.palette {
                Palette::Blue => theme.ansi_fg_blue(),
                Palette::Green => theme.ansi_fg_green(),
                Palette::Orange => theme.ansi_fg_yellow(),
                Palette::Purple => theme.ansi_fg_magenta(),
                Palette::Red => theme.ansi_fg_red(),
            };
            (content, background)
        }
        None => (
            AvatarContent::DisplayName(fallback_name.to_string()),
            theme.ansi_fg_blue(),
        ),
    };
    Avatar::new(
        content,
        UiComponentStyles {
            width: Some(SHELL_AVATAR_SIZE),
            height: Some(SHELL_AVATAR_SIZE),
            font_size: Some(11.),
            font_family_id: Some(appearance.monospace_font_family()),
            font_weight: Some(Weight::Bold),
            font_color: Some(theme.surface_1().into()),
            background: Some(background.into()),
            border_radius: Some(CornerRadius::with_all(Radius::Pixels(
                SHELL_AVATAR_SIZE / 2.,
            ))),
            ..Default::default()
        },
    )
    .build()
    .finish()
}

fn status_color(
    status: AgentTabStatus,
    completion_flash: bool,
    theme: &crate::themes::theme::WarpTheme,
) -> pathfinder_color::ColorU {
    if completion_flash {
        return theme.ansi_fg_green();
    }
    match status {
        AgentTabStatus::Working => theme.ansi_fg_blue(),
        AgentTabStatus::Waiting => theme.ansi_fg_yellow(),
        AgentTabStatus::Blocked => theme.ansi_fg_red(),
        AgentTabStatus::Failed => theme.ansi_fg_red(),
        AgentTabStatus::Completed => theme.ansi_fg_green(),
        AgentTabStatus::Unavailable => theme.nonactive_ui_text_color().into(),
    }
}

/// Compact row: status dot + title + one meta line (Herdr sidebar density).
#[allow(clippy::too_many_arguments)]
fn nav_card(
    ms: MouseStateHandle,
    title: String,
    subtitle: String,
    selected: bool,
    accent: pathfinder_color::ColorU,
    appearance: &Appearance,
    theme: &crate::themes::theme::WarpTheme,
    action: WorkspaceAction,
    trailing_action: Option<(String, WorkspaceAction)>,
    kind: NavCardKind,
) -> Box<dyn Element> {
    let main = theme.main_text_color(theme.background());
    let sub = theme.sub_text_color(theme.background());
    let font = appearance.ui_font_family();
    let accent_for_kind = match kind {
        NavCardKind::Danger => theme.ansi_fg_red(),
        NavCardKind::Warning => theme.ansi_fg_yellow(),
        NavCardKind::Neutral => theme.nonactive_ui_text_color().into(),
        NavCardKind::Agent | NavCardKind::History => accent,
    };
    Hoverable::new(ms, move |mouse| {
        let bg = if selected {
            Some(internal_colors::fg_overlay_2(theme))
        } else if mouse.is_hovered() {
            Some(internal_colors::fg_overlay_1(theme))
        } else {
            None
        };
        let title_color: pathfinder_color::ColorU = match kind {
            NavCardKind::Danger => theme.ansi_fg_red(),
            _ => main.into(),
        };
        let mut title_row = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(8.)
            .with_child(status_dot(accent_for_kind))
            .with_child(
                Text::new_inline(title.clone(), font, 12.)
                    .with_clip(ClipConfig::ellipsis())
                    .with_color(title_color)
                    .finish(),
            );
        if let Some((label, act)) = trailing_action.clone() {
            title_row = title_row
                .with_child(Expanded::new(1., Empty::new().finish()).finish())
                .with_child(mini_chip(&label, act, appearance, theme));
        }

        let mut text_col = Flex::column()
            .with_main_axis_size(MainAxisSize::Min)
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_spacing(2.)
            .with_child(title_row.finish());
        if !subtitle.is_empty() {
            // Indent under title so the status dot stays the glance cue.
            let meta = subtitle.lines().take(1).collect::<Vec<_>>().join("");
            text_col = text_col.with_child(
                Container::new(
                    Text::new_inline(meta, font, 10.)
                        .with_clip(ClipConfig::ellipsis())
                        .with_color(sub.into())
                        .finish(),
                )
                .with_padding(Padding::uniform(0.).with_left(STATUS_DOT + 8.))
                .finish(),
            );
        }

        let mut c = Container::new(text_col.finish()).with_padding(
            Padding::uniform(0.)
                .with_top(6.)
                .with_bottom(6.)
                .with_left(6.)
                .with_right(6.),
        );
        if let Some(bg) = bg {
            c = c.with_background(bg);
        }
        if selected {
            c = c
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(CARD_RADIUS)))
                .with_border(Border::all(1.).with_border_fill(accent_for_kind));
        } else {
            c = c.with_corner_radius(CornerRadius::with_all(Radius::Pixels(CARD_RADIUS)));
        }
        c.finish()
    })
    .with_cursor(Cursor::PointingHand)
    .on_click(move |ctx, _, _| {
        ctx.dispatch_typed_action(action.clone());
    })
    .finish()
}

/// Compact toggle pill for filter toolbars.
fn filter_pill(
    ms: MouseStateHandle,
    label: &str,
    active: bool,
    appearance: &Appearance,
    theme: &crate::themes::theme::WarpTheme,
    action: WorkspaceAction,
) -> Box<dyn Element> {
    let label = label.to_string();
    let font = appearance.ui_font_family();
    let main = theme.main_text_color(theme.background());
    let sub = theme.sub_text_color(theme.background());
    Hoverable::new(ms, move |mouse| {
        let (bg, fg, border) = if active {
            (
                internal_colors::fg_overlay_2(theme),
                main,
                theme.accent(),
            )
        } else if mouse.is_hovered() {
            (
                internal_colors::fg_overlay_1(theme),
                main,
                theme.outline(),
            )
        } else {
            (
                internal_colors::fg_overlay_1(theme),
                sub,
                theme.outline(),
            )
        };
        Container::new(
            Text::new_inline(label.clone(), font, 10.)
                .with_color(fg.into())
                .finish(),
        )
        .with_padding(
            Padding::uniform(0.)
                .with_top(4.)
                .with_bottom(4.)
                .with_left(8.)
                .with_right(8.),
        )
        .with_background(bg)
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(PILL_RADIUS)))
        .with_border(Border::all(1.).with_border_fill(border))
        .finish()
    })
    .with_cursor(Cursor::PointingHand)
    .on_click(move |ctx, _, _| {
        ctx.dispatch_typed_action(action.clone());
    })
    .finish()
}

/// Tiny inline chip used as trailing action on nav cards.
fn mini_chip(
    label: &str,
    action: WorkspaceAction,
    appearance: &Appearance,
    theme: &crate::themes::theme::WarpTheme,
) -> Box<dyn Element> {
    let label = label.to_string();
    let sub = theme.sub_text_color(theme.background());
    Hoverable::new(MouseStateHandle::default(), move |mouse| {
        let bg = if mouse.is_hovered() {
            internal_colors::fg_overlay_2(theme)
        } else {
            internal_colors::fg_overlay_1(theme)
        };
        Container::new(
            Text::new_inline(label.clone(), appearance.ui_font_family(), 10.)
                .with_color(sub.into())
                .finish(),
        )
        .with_padding(
            Padding::uniform(0.)
                .with_top(2.)
                .with_bottom(2.)
                .with_left(6.)
                .with_right(6.),
        )
        .with_background(bg)
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(PILL_RADIUS)))
        .finish()
    })
    .with_cursor(Cursor::PointingHand)
    .on_click(move |ctx, _, _| {
        ctx.dispatch_typed_action(action.clone());
    })
    .finish()
}

fn render_codex_detail_column(
    detail: &SubagentDetail,
    terminal_view_id: EntityId,
    child_key: &str,
    confirm_remove: bool,
    confirm_stop: bool,
    is_history: bool,
    show_stop: bool,
    show_technical: bool,
    profile: AgentUiProfile,
    detail_scroll: warpui::elements::ClippedScrollStateHandle,
    app: &AppContext,
) -> Box<dyn Element> {
    let appearance = Appearance::as_ref(app);
    let theme = appearance.theme();
    let main = theme.main_text_color(theme.background());
    let sub = theme.sub_text_color(theme.background());
    let font = appearance.ui_font_family();
    let accent = status_color(detail.status, false, theme);

    // —— Header ——
    let mut meta_bits = vec![detail.status_label.clone(), detail.parent_label.clone()];
    if let Some(e) = &detail.elapsed_label {
        meta_bits.push(e.clone());
    }
    let mut header_inner = Flex::column()
        .with_main_axis_size(MainAxisSize::Min)
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(6.)
        .with_child(
            Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_spacing(8.)
                .with_child(
                    Text::new_inline(detail.display_name.clone(), font, 16.)
                        .with_color(main.into())
                        .finish(),
                )
                .with_child(status_badge(
                    AgentUiProfile::status_chip(detail.status, false),
                    accent,
                    font,
                    theme,
                ))
                .finish(),
        )
        .with_child(
            Text::new_inline(
                format!(
                    "{} · {}",
                    profile.short_name,
                    meta_bits.join(" · ")
                ),
                font,
                11.,
            )
            .with_color(sub.into())
            .finish(),
        );
    if detail.breadcrumb.len() > 1 {
        header_inner = header_inner.with_child(
            Text::new_inline(detail.breadcrumb_string(), font, 10.)
                .with_color(sub.into())
                .with_clip(ClipConfig::ellipsis())
                .finish(),
        );
    }
    if let Some(act) = &detail.activity {
        header_inner = header_inner.with_child(
            Text::new_inline(format!("Ahora · {act}"), font, 11.)
                .with_clip(ClipConfig::ellipsis())
                .with_color(main.into())
                .finish(),
        );
    }
    let header = detail_section(header_inner.finish(), accent, theme);

    let mut sections = Flex::column()
        .with_main_axis_size(MainAxisSize::Min)
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(10.);

    if let Some(task) = &detail.task_summary {
        sections = sections.with_child(detail_section(
            section_body("Objetivo", task, font, main, sub),
            theme.ansi_fg_blue(),
            theme,
        ));
    }

    // —— Activity timeline ——
    use crate::workspace::codex_session_shell::{group_timeline_actions, partition_activity_feed};
    let raw_lines: Vec<String> = if !detail.transcript_lines.is_empty() {
        detail
            .transcript_lines
            .iter()
            .rev()
            .take(DETAIL_FEED_LINES)
            .rev()
            .cloned()
            .collect()
    } else {
        detail
            .tools
            .iter()
            .rev()
            .take(DETAIL_TOOL_LINES)
            .rev()
            .cloned()
            .collect()
    };
    let (primary_feed, technical_feed) = partition_activity_feed(&raw_lines);
    let groups = group_timeline_actions(&primary_feed);

    let mut feed = Flex::column()
        .with_main_axis_size(MainAxisSize::Min)
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(4.)
        .with_child(section_title("Actividad", font, main));
    if groups.is_empty() {
        feed = feed.with_child(
            Text::new_inline(
                "Sin eventos todavía. Cuando el subagent use tools o escriba, aparecen acá.",
                font,
                11.,
            )
            .with_color(sub.into())
            .finish(),
        );
    } else {
        for (title, kids) in &groups {
            if kids.len() > 1 {
                feed = feed.with_child(
                    Text::new_inline(format!("▸ {title}"), font, 11.)
                        .with_color(main.into())
                        .finish(),
                );
                for child_line in kids {
                    feed = feed.with_child(
                        Text::new_inline(format!("   {child_line}"), font, 10.)
                            .with_color(sub.into())
                            .with_clip(ClipConfig::ellipsis())
                            .finish(),
                    );
                }
            } else {
                let line = kids.first().cloned().unwrap_or_else(|| title.clone());
                feed = feed.with_child(
                    Text::new_inline(line, font, 11.)
                        .with_color(sub.into())
                        .with_clip(ClipConfig::ellipsis())
                        .finish(),
                );
            }
        }
    }
    sections = sections.with_child(detail_section(feed.finish(), theme.ansi_fg_blue(), theme));

    // —— Files ——
    let files_body = if detail.files.is_empty() {
        "No se realizaron cambios en archivos.".into()
    } else {
        detail
            .files
            .iter()
            .rev()
            .take(16)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    };
    sections = sections.with_child(detail_section(
        section_body("En el repo", &files_body, font, main, sub),
        theme.ansi_fg_green(),
        theme,
    ));

    if let Some(result) = &detail.result_summary {
        sections = sections.with_child(detail_section(
            section_body("Resultado", result, font, main, main),
            theme.ansi_fg_green(),
            theme,
        ));
    }

    // Technical details collapsible
    let tech_count = technical_feed.len()
        + detail
            .tools
            .iter()
            .filter(|t| t.contains('{') || t.len() > 100)
            .count();
    let tech_label = if show_technical {
        format!("▾ Detalles técnicos ({tech_count})")
    } else {
        format!("▸ Detalles técnicos ({tech_count})")
    };
    let mut tech = Flex::column()
        .with_spacing(4.)
        .with_child(action_chip(
            &tech_label,
            WorkspaceAction::CodexShellToggleTechnicalDetails { terminal_view_id },
            appearance,
            theme,
            ChipTone::Secondary,
        ));
    if show_technical {
        for line in technical_feed.iter().chain(detail.tools.iter()).take(24) {
            tech = tech.with_child(
                Text::new_inline(line.clone(), font, 9.)
                    .with_color(sub.into())
                    .with_clip(ClipConfig::ellipsis())
                    .finish(),
            );
        }
    }
    sections = sections.with_child(detail_section(
        tech.finish(),
        theme.nonactive_ui_text_color().into(),
        theme,
    ));

    // —— Actions ——
    let mut actions = Flex::row()
        .with_main_axis_size(MainAxisSize::Max)
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_spacing(8.)
        .with_child(action_chip(
            profile.back_to_parent,
            WorkspaceAction::CodexShellSelectParent { terminal_view_id },
            appearance,
            theme,
            ChipTone::Primary,
        ))
        .with_child(action_chip(
            "Cerrar vista",
            WorkspaceAction::CodexShellCloseView { terminal_view_id },
            appearance,
            theme,
            ChipTone::Secondary,
        ));

    if !is_history {
        let remove_label = if confirm_remove {
            "¿Quitar? Tocá de nuevo"
        } else {
            "Quitar de la lista"
        };
        actions = actions.with_child(action_chip(
            remove_label,
            WorkspaceAction::CodexShellRemoveFromList {
                terminal_view_id,
                child_key: child_key.to_string(),
            },
            appearance,
            theme,
            if confirm_remove {
                ChipTone::Danger
            } else {
                ChipTone::Secondary
            },
        ));

        let is_working = matches!(
            detail.status,
            AgentTabStatus::Working
                | AgentTabStatus::Waiting
                | AgentTabStatus::Unavailable
        );
        if show_stop && is_working {
            let stop_label = if confirm_stop {
                "¿Detener? Tocá de nuevo"
            } else {
                "Detener"
            };
            actions = actions.with_child(action_chip(
                stop_label,
                WorkspaceAction::CodexShellStop {
                    terminal_view_id,
                    child_key: child_key.to_string(),
                },
                appearance,
                theme,
                ChipTone::Danger,
            ));
        }
    }

    let scroll = ClippedScrollable::vertical(
        detail_scroll,
        sections.finish(),
        ScrollbarWidth::Custom(3.),
        theme.nonactive_ui_detail().into(),
        theme.active_ui_detail().into(),
        ElementFill::None,
    )
    .with_overlayed_scrollbar()
    .finish();

    let body = Flex::column()
        .with_main_axis_size(MainAxisSize::Max)
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(12.)
        .with_child(header)
        .with_child(Shrinkable::new(1., scroll).finish())
        .with_child(actions.finish())
        .finish();

    Container::new(body)
        .with_uniform_padding(14.)
        .finish()
}

fn section_title(
    text: &str,
    font: warpui::fonts::FamilyId,
    color: impl Into<pathfinder_color::ColorU> + Copy,
) -> Box<dyn Element> {
    Text::new_inline(text.to_string(), font, 11.)
        .with_color(color.into())
        .finish()
}

fn section_body(
    title: &str,
    body: &str,
    font: warpui::fonts::FamilyId,
    title_color: impl Into<pathfinder_color::ColorU> + Copy,
    body_color: impl Into<pathfinder_color::ColorU> + Copy,
) -> Box<dyn Element> {
    Flex::column()
        .with_main_axis_size(MainAxisSize::Min)
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(4.)
        .with_child(section_title(title, font, title_color))
        .with_child(
            Text::new_inline(body.to_string(), font, 12.)
                .with_color(body_color.into())
                .finish(),
        )
        .finish()
}

fn detail_section(
    inner: Box<dyn Element>,
    accent: pathfinder_color::ColorU,
    theme: &crate::themes::theme::WarpTheme,
) -> Box<dyn Element> {
    Container::new(inner)
        .with_uniform_padding(12.)
        .with_background(internal_colors::fg_overlay_1(theme))
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(CARD_RADIUS)))
        .with_border(Border::left(ACCENT_BAR_W).with_border_fill(accent))
        .finish()
}

fn status_badge(
    label: &str,
    accent: pathfinder_color::ColorU,
    font: warpui::fonts::FamilyId,
    theme: &crate::themes::theme::WarpTheme,
) -> Box<dyn Element> {
    Container::new(
        Text::new_inline(label.to_string(), font, 10.)
            .with_color(accent.into())
            .finish(),
    )
    .with_padding(
        Padding::uniform(0.)
            .with_top(3.)
            .with_bottom(3.)
            .with_left(8.)
            .with_right(8.),
    )
    .with_background(internal_colors::fg_overlay_2(theme))
    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(PILL_RADIUS)))
    .with_border(Border::all(1.).with_border_fill(accent))
    .finish()
}

#[derive(Clone, Copy)]
enum ChipTone {
    Primary,
    Secondary,
    Danger,
}

fn render_history_detail_column(
    entry: &crate::workspace::codex_session_shell::HistorySubagentEntry,
    terminal_view_id: EntityId,
    profile: AgentUiProfile,
    detail_scroll: warpui::elements::ClippedScrollStateHandle,
    app: &AppContext,
) -> Box<dyn Element> {
    use crate::workspace::codex_session_shell::{
        format_duration_ms, format_finished_at_ms as fmt_done, format_history_copy_result,
        format_history_copy_summary,
    };

    let appearance = Appearance::as_ref(app);
    let theme = appearance.theme();
    let main = theme.main_text_color(theme.background());
    let sub = theme.sub_text_color(theme.background());
    let font = appearance.ui_font_family();
    let accent = if !entry.errors.is_empty()
        || matches!(
            entry.status,
            crate::workspace::codex_session_shell::HistoryStatus::Failed
                | crate::workspace::codex_session_shell::HistoryStatus::Blocked
        )
    {
        theme.ansi_fg_red()
    } else {
        theme.ansi_fg_green()
    };

    let header_inner = Flex::column()
        .with_main_axis_size(MainAxisSize::Min)
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(6.)
        .with_child(
            Flex::row()
                .with_spacing(8.)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(
                    Text::new_inline(entry.display_name.clone(), font, 16.)
                        .with_color(main.into())
                        .finish(),
                )
                .with_child(status_badge("historial", accent, font, theme))
                .finish(),
        )
        .with_child(
            Text::new_inline(
                format!(
                    "{} · {} · {} · {} · {}",
                    profile.short_name,
                    entry.status_label,
                    entry.platform,
                    entry.parent_label,
                    format_duration_ms(entry.started_at_ms, entry.finished_at_ms)
                ),
                font,
                11.,
            )
            .with_color(sub.into())
            .finish(),
        )
        .with_child(
            Text::new_inline(fmt_done(entry.finished_at_ms), font, 10.)
                .with_color(sub.into())
                .finish(),
        );

    let mut sections = Flex::column()
        .with_main_axis_size(MainAxisSize::Max)
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(10.)
        .with_child(detail_section(header_inner.finish(), accent, theme));

    if let Some(obj) = entry.objective.as_ref().or(entry.task_summary.as_ref()) {
        sections = sections.with_child(detail_section(
            section_body("Objetivo original", obj, font, main, sub),
            theme.ansi_fg_blue(),
            theme,
        ));
    }
    if let Some(work) = &entry.work_summary {
        sections = sections.with_child(detail_section(
            section_body("Trabajo realizado", work, font, main, sub),
            theme.ansi_fg_blue(),
            theme,
        ));
    }
    if !entry.activity_highlights.is_empty() {
        let mut tl = Flex::column()
            .with_spacing(3.)
            .with_child(section_title("Timeline", font, main));
        for line in entry.activity_highlights.iter().rev().take(40).rev() {
            tl = tl.with_child(
                Text::new_inline(line.clone(), font, 11.)
                    .with_color(sub.into())
                    .with_clip(ClipConfig::ellipsis())
                    .finish(),
            );
        }
        sections = sections.with_child(detail_section(
            tl.finish(),
            theme.ansi_fg_blue(),
            theme,
        ));
    }
    if !entry.tools_used.is_empty() {
        sections = sections.with_child(detail_section(
            section_body(
                "Herramientas",
                &entry
                    .tools_used
                    .iter()
                    .take(12)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" · "),
                font,
                main,
                sub,
            ),
            theme.nonactive_ui_text_color().into(),
            theme,
        ));
    }
    let files_body = if entry.files_changed.is_empty() {
        "No se realizaron cambios en archivos.".into()
    } else {
        entry.files_changed.join("\n")
    };
    sections = sections.with_child(detail_section(
        section_body("En el repo", &files_body, font, main, sub),
        theme.ansi_fg_green(),
        theme,
    ));
    if let Some(result) = entry.result_summary.as_ref().or(entry.parent_handoff.as_ref()) {
        sections = sections.with_child(detail_section(
            section_body("Resultado", result, font, main, main),
            theme.ansi_fg_green(),
            theme,
        ));
    }
    if !entry.errors.is_empty() {
        sections = sections.with_child(detail_section(
            section_body("Errores", &entry.errors.join(" · "), font, theme.ansi_fg_red(), theme.ansi_fg_red()),
            theme.ansi_fg_red(),
            theme,
        ));
    }
    if !entry.pending.is_empty() {
        sections = sections.with_child(detail_section(
            section_body(
                "Pendientes",
                &entry.pending.join(" · "),
                font,
                theme.ansi_fg_yellow(),
                theme.ansi_fg_yellow(),
            ),
            theme.ansi_fg_yellow(),
            theme,
        ));
    }

    let summary = format_history_copy_summary(entry);
    let result_copy = format_history_copy_result(entry);
    let actions = Flex::row()
        .with_spacing(8.)
        .with_child(action_chip(
            profile.back_to_parent,
            WorkspaceAction::CodexShellSelectParent { terminal_view_id },
            appearance,
            theme,
            ChipTone::Primary,
        ))
        .with_child(action_chip(
            "Copiar resumen",
            WorkspaceAction::CopyTextToClipboard(summary),
            appearance,
            theme,
            ChipTone::Secondary,
        ))
        .with_child(action_chip(
            "Copiar resultado",
            WorkspaceAction::CopyTextToClipboard(result_copy),
            appearance,
            theme,
            ChipTone::Secondary,
        ))
        .with_child(action_chip(
            "Eliminar",
            WorkspaceAction::CodexShellDeleteHistory {
                terminal_view_id,
                history_id: entry.id.clone(),
            },
            appearance,
            theme,
            ChipTone::Danger,
        ))
        .finish();

    let scroll = ClippedScrollable::vertical(
        detail_scroll,
        sections.finish(),
        ScrollbarWidth::Custom(3.),
        theme.nonactive_ui_detail().into(),
        theme.active_ui_detail().into(),
        ElementFill::None,
    )
    .with_overlayed_scrollbar()
    .finish();

    Container::new(
        Flex::column()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_spacing(10.)
            .with_child(Shrinkable::new(1., scroll).finish())
            .with_child(actions)
            .finish(),
    )
    .with_uniform_padding(14.)
    .finish()
}

fn action_chip(
    label: &str,
    action: WorkspaceAction,
    appearance: &Appearance,
    theme: &crate::themes::theme::WarpTheme,
    tone: ChipTone,
) -> Box<dyn Element> {
    let label = label.to_string();
    let font = appearance.ui_font_family();
    let main = theme.main_text_color(theme.background());
    Hoverable::new(MouseStateHandle::default(), move |mouse| {
        let hovered = mouse.is_hovered();
        let bg = match (tone, hovered) {
            (ChipTone::Primary, _) | (_, true) => internal_colors::fg_overlay_2(theme),
            _ => internal_colors::fg_overlay_1(theme),
        };
        let fg: pathfinder_color::ColorU = match tone {
            ChipTone::Danger => theme.ansi_fg_red(),
            _ => main.into(),
        };
        let border = match tone {
            ChipTone::Primary => theme.accent(),
            ChipTone::Danger if hovered => theme.ansi_fg_red().into(),
            _ => theme.outline(),
        };
        Container::new(
            Text::new_inline(label.clone(), font, 11.)
                .with_color(fg)
                .finish(),
        )
        .with_padding(
            Padding::uniform(0.)
                .with_top(6.)
                .with_bottom(6.)
                .with_left(10.)
                .with_right(10.),
        )
        .with_background(bg)
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(PILL_RADIUS)))
        .with_border(Border::all(1.).with_border_fill(border))
        .finish()
    })
    .with_cursor(Cursor::PointingHand)
    .on_click(move |ctx, _, _| {
        ctx.dispatch_typed_action(action.clone());
    })
    .finish()
}
