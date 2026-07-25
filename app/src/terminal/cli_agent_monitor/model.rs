//! Singleton model wrapping [`MonitorStore`] for the app entity system.

use warp_core::paths::data_dir;
use warpui::{Entity, ModelContext, SingletonEntity};

use super::adapters::MonitorAgentKind;
use super::alerts::{AlertPreferences, alerts_prefs_path};
use super::pet::{PetController, PetMode, pet_prefs_path};
use super::session::{LiveSessionSignal, MonitorSession, NavigationResult};
use super::store::MonitorStore;
use super::ui::{PanelRow, compact_indicator_from_store, panel_rows_from_store};

/// Alert shown on the floating desktop pet (and mirrored to OS notifications).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PetAlert {
    pub title: String,
    pub body: String,
    pub session_id: String,
}

pub enum CliAgentMonitorEvent {
    Changed,
    /// Pet visibility changed (close / show / minimize). Monitor sessions untouched.
    PetModeChanged { mode: PetMode },
    NotificationRequested {
        title: String,
        body: String,
        session_id: String,
        kind: super::store::NotificationKind,
    },
}

/// App-wide CLI agent monitor (ADHD review memory) + pet visibility prefs.
pub struct CliAgentMonitorModel {
    store: MonitorStore,
    pet: PetController,
    alerts: AlertPreferences,
    alerts_path: std::path::PathBuf,
    /// When true, the compact panel is open.
    panel_open: bool,
    /// Sticky alert on the floating pet until the user views/activates it.
    active_alert: Option<PetAlert>,
}

impl CliAgentMonitorModel {
    pub fn new() -> Self {
        let dir = data_dir().join("cli_agent_monitor");
        let path = dir.join("sessions.json");
        let store = if path.exists() {
            MonitorStore::load_from_path(&path).unwrap_or_else(|err| {
                log::warn!("cli_agent_monitor: failed to load store: {err}");
                MonitorStore::with_path(path)
            })
        } else {
            MonitorStore::with_path(path)
        };
        let pet = PetController::with_path(pet_prefs_path(data_dir()));
        let alerts_path = alerts_prefs_path(data_dir());
        let alerts = AlertPreferences::load_or_default(&alerts_path).with_env_overrides();
        Self {
            store,
            pet,
            alerts,
            alerts_path,
            panel_open: false,
            active_alert: None,
        }
    }

    pub fn store(&self) -> &MonitorStore {
        &self.store
    }

    pub fn pet(&self) -> &PetController {
        &self.pet
    }

    pub fn pet_mode(&self) -> PetMode {
        self.pet.mode()
    }

    pub fn alerts(&self) -> &AlertPreferences {
        &self.alerts
    }

    pub fn set_desktop_alerts_enabled(&mut self, enabled: bool, ctx: &mut ModelContext<Self>) {
        self.alerts.desktop_enabled = enabled;
        let _ = self.alerts.save_to_path(&self.alerts_path);
        ctx.emit(CliAgentMonitorEvent::Changed);
        ctx.notify();
    }

    pub fn set_whatsapp_alerts_enabled(&mut self, enabled: bool, ctx: &mut ModelContext<Self>) {
        self.alerts.whatsapp_enabled = enabled;
        let _ = self.alerts.save_to_path(&self.alerts_path);
        ctx.emit(CliAgentMonitorEvent::Changed);
        ctx.notify();
    }

    pub fn set_whatsapp_credentials(
        &mut self,
        phone: String,
        api_key: String,
        ctx: &mut ModelContext<Self>,
    ) {
        self.alerts.whatsapp_phone = phone;
        self.alerts.whatsapp_api_key = api_key;
        let _ = self.alerts.save_to_path(&self.alerts_path);
        ctx.emit(CliAgentMonitorEvent::Changed);
        ctx.notify();
    }

    pub fn reload_alerts_from_disk(&mut self, ctx: &mut ModelContext<Self>) {
        self.alerts =
            AlertPreferences::load_or_default(&self.alerts_path).with_env_overrides();
        ctx.emit(CliAgentMonitorEvent::Changed);
        ctx.notify();
    }

    /// Sticky pet speech-bubble alert (cleared when the user views the session).
    pub fn active_alert(&self) -> Option<&PetAlert> {
        self.active_alert.as_ref()
    }

    pub fn set_active_alert(&mut self, alert: Option<PetAlert>, ctx: &mut ModelContext<Self>) {
        self.active_alert = alert;
        ctx.emit(CliAgentMonitorEvent::Changed);
        ctx.notify();
    }

    pub fn clear_active_alert(&mut self, ctx: &mut ModelContext<Self>) {
        if self.active_alert.take().is_some() {
            ctx.emit(CliAgentMonitorEvent::Changed);
            ctx.notify();
        }
    }

    /// Hide floating pet. Does **not** review sessions or stop monitoring.
    pub fn close_pet(&mut self, ctx: &mut ModelContext<Self>) {
        self.pet.close_pet();
        // Always clear the speech bubble so the UI cannot get stuck open.
        self.active_alert = None;
        ctx.emit(CliAgentMonitorEvent::PetModeChanged {
            mode: PetMode::Closed,
        });
        ctx.emit(CliAgentMonitorEvent::Changed);
        ctx.notify();
    }

    pub fn minimize_pet(&mut self, ctx: &mut ModelContext<Self>) {
        self.pet.minimize_pet();
        ctx.emit(CliAgentMonitorEvent::PetModeChanged {
            mode: PetMode::Minimized,
        });
        ctx.emit(CliAgentMonitorEvent::Changed);
        ctx.notify();
    }

    /// Show floating pet again; pending sessions remain as-is (still green if unseen).
    pub fn show_pet(&mut self, ctx: &mut ModelContext<Self>) {
        self.pet.show_pet();
        ctx.emit(CliAgentMonitorEvent::PetModeChanged {
            mode: PetMode::Visible,
        });
        ctx.emit(CliAgentMonitorEvent::Changed);
        ctx.notify();
    }

    pub fn may_show_floating_pet(&self) -> bool {
        self.pet.may_show_floating_ui()
    }

    pub fn panel_open(&self) -> bool {
        self.panel_open
    }

    pub fn toggle_panel(&mut self, ctx: &mut ModelContext<Self>) {
        self.panel_open = !self.panel_open;
        ctx.emit(CliAgentMonitorEvent::Changed);
        ctx.notify();
    }

    pub fn set_panel_open(&mut self, open: bool, ctx: &mut ModelContext<Self>) {
        self.panel_open = open;
        ctx.emit(CliAgentMonitorEvent::Changed);
        ctx.notify();
    }

    pub fn indicator_text(&self) -> String {
        compact_indicator_from_store(&self.store)
    }

    pub fn panel_rows(
        &self,
        now_ms: u64,
        terminal_exists: impl Fn(&MonitorSession) -> bool,
    ) -> Vec<PanelRow> {
        panel_rows_from_store(&self.store, now_ms, terminal_exists)
    }

    pub fn upsert_session(&mut self, session: MonitorSession, ctx: &mut ModelContext<Self>) {
        self.store.upsert_session(session);
        let _ = self.store.persist();
        ctx.emit(CliAgentMonitorEvent::Changed);
        ctx.notify();
    }

    pub fn apply_live_signal(
        &mut self,
        id: &str,
        signal: LiveSessionSignal,
        now_ms: u64,
        ctx: &mut ModelContext<Self>,
    ) {
        if self.store.apply_live_signal(id, signal, now_ms) {
            for note in self.store.take_pending_notifications() {
                // Sticky bubble on the floating pet until the user opens the session.
                self.active_alert = Some(PetAlert {
                    title: note.title.clone(),
                    body: note.body.clone(),
                    session_id: note.session_id.clone(),
                });
                // Surface the pet when an agent finishes so the alert is visible.
                if !self.pet.is_floating_visible() {
                    self.pet.show_pet();
                    ctx.emit(CliAgentMonitorEvent::PetModeChanged {
                        mode: PetMode::Visible,
                    });
                }
                let _ = self.pet.on_important_alert();
                ctx.emit(CliAgentMonitorEvent::NotificationRequested {
                    title: note.title,
                    body: note.body,
                    session_id: note.session_id,
                    kind: note.kind,
                });
            }
            let _ = self.store.persist();
            ctx.emit(CliAgentMonitorEvent::Changed);
            ctx.notify();
        }
    }

    pub fn activate(
        &mut self,
        id: &str,
        navigation: NavigationResult,
        now_ms: u64,
        ctx: &mut ModelContext<Self>,
    ) {
        self.store.activate_session(id, navigation, now_ms);
        if self
            .active_alert
            .as_ref()
            .is_some_and(|a| a.session_id == id)
        {
            self.active_alert = None;
        }
        let _ = self.store.persist();
        ctx.emit(CliAgentMonitorEvent::Changed);
        ctx.notify();
    }

    pub fn mark_reviewed(&mut self, id: &str, now_ms: u64, ctx: &mut ModelContext<Self>) {
        self.store.mark_reviewed(id, now_ms);
        if self
            .active_alert
            .as_ref()
            .is_some_and(|a| a.session_id == id)
        {
            self.active_alert = None;
        }
        let _ = self.store.persist();
        ctx.emit(CliAgentMonitorEvent::Changed);
        ctx.notify();
    }

    /// Product path: focus result → review transition (used by Workspace).
    pub fn product_activate(
        &mut self,
        id: &str,
        terminal_exists: bool,
        focus_succeeded: bool,
        now_ms: u64,
        ctx: &mut ModelContext<Self>,
    ) -> Option<super::product::ActivationOutcome> {
        let outcome = super::product::product_activate_session(
            &mut self.store,
            id,
            terminal_exists,
            focus_succeeded,
            now_ms,
        );
        if self
            .active_alert
            .as_ref()
            .is_some_and(|a| a.session_id == id)
        {
            self.active_alert = None;
        }
        let _ = self.store.persist();
        ctx.emit(CliAgentMonitorEvent::Changed);
        ctx.notify();
        outcome
    }

    /// Product path: explicit “Marcar como revisado”.
    pub fn product_mark_reviewed(
        &mut self,
        id: &str,
        now_ms: u64,
        ctx: &mut ModelContext<Self>,
    ) -> Option<super::state::MonitorState> {
        let state = super::product::product_mark_reviewed(&mut self.store, id, now_ms);
        if self
            .active_alert
            .as_ref()
            .is_some_and(|a| a.session_id == id)
        {
            self.active_alert = None;
        }
        let _ = self.store.persist();
        ctx.emit(CliAgentMonitorEvent::Changed);
        ctx.notify();
        state
    }

    /// Ensure a session exists for a live CLI agent pane, then apply status.
    pub fn track_cli_session(
        &mut self,
        session_id: String,
        agent: MonitorAgentKind,
        terminal_view_id: String,
        project: Option<String>,
        cwd: Option<String>,
        signal: LiveSessionSignal,
        now_ms: u64,
        ctx: &mut ModelContext<Self>,
    ) {
        // Collapse synthetic `Codex:123` rows once the plugin UUID arrives.
        self.store
            .reconcile_for_terminal(&session_id, &terminal_view_id);
        if let Some(existing) = self.store.get_mut(&session_id) {
            existing.terminal_view_id = Some(terminal_view_id);
            if project.is_some() {
                existing.project = project;
            }
            if cwd.is_some() {
                existing.cwd = cwd;
            }
            existing.updated_at_ms = now_ms;
        } else {
            let mut session =
                MonitorSession::new(session_id.clone(), agent, now_ms, Some(terminal_view_id));
            session.project = project;
            session.cwd = cwd;
            self.store.upsert_session(session);
        }
        self.apply_live_signal(&session_id, signal, now_ms, ctx);
    }

    /// Drop the sticky pet bubble without marking the session reviewed.
    pub fn dismiss_active_alert(&mut self, ctx: &mut ModelContext<Self>) {
        self.clear_active_alert(ctx);
    }

    /// User explicitly removes one row (panel “Quitar”).
    pub fn remove_session(&mut self, id: &str, ctx: &mut ModelContext<Self>) {
        if self
            .active_alert
            .as_ref()
            .is_some_and(|a| a.session_id == id)
        {
            self.active_alert = None;
        }
        if self.store.remove_session(id) {
            let _ = self.store.persist();
            ctx.emit(CliAgentMonitorEvent::Changed);
            ctx.notify();
        }
    }

    /// User clears the whole monitor list.
    pub fn clear_all_sessions(&mut self, ctx: &mut ModelContext<Self>) {
        self.active_alert = None;
        if self.store.clear_all() > 0 {
            let _ = self.store.persist();
        }
        ctx.emit(CliAgentMonitorEvent::Changed);
        ctx.notify();
    }

    /// CLI process ended for this terminal view — drop zombies, keep pending review.
    pub fn on_terminal_ended(&mut self, terminal_view_id: &str, ctx: &mut ModelContext<Self>) {
        let removed = self.store.on_terminal_ended(terminal_view_id);
        // Also drop orphans with no terminal (synthetic leftovers).
        let orphans = self.store.prune_orphan_live_sessions();
        if removed + orphans > 0 {
            let _ = self.store.persist();
            ctx.emit(CliAgentMonitorEvent::Changed);
            ctx.notify();
        }
    }
}

impl Default for CliAgentMonitorModel {
    fn default() -> Self {
        Self::new()
    }
}

impl Entity for CliAgentMonitorModel {
    type Event = CliAgentMonitorEvent;
}

impl SingletonEntity for CliAgentMonitorModel {}
