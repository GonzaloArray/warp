//! Permanent CLI agent monitor (Claude / Codex / Grok) with ADHD review persistence.
//!
//! Pure domain + presentation helpers live here so unit tests drive shipped logic
//! without a live desktop. UI surfaces project [`MonitorStore`] snapshots.

#![allow(dead_code)] // Public surface for UI/nav wiring and future adapters.

pub mod adapters;
pub mod alerts;
pub mod avatar_toast;
pub mod bridge;
pub mod desktop_pet;
pub mod model;
pub mod panel;
pub mod panel_view;
pub mod pet;
pub mod product;
pub mod session;
pub mod sound;
pub mod state;
pub mod store;
pub mod ui;

pub use adapters::{
    AdapterRegistry, CustomAdapter, MonitorAgentKind, ProcessSignal, session_identity_key,
};
pub use avatar_toast::{
    CliAgentAvatarToastEvent, CliAgentAvatarToastStack, avatar_toast_positioning,
};
pub use desktop_pet::{
    DesktopPetView, DesktopPetWindowRegistry, close_desktop_pet_window, ensure_desktop_pet_window,
    sync_desktop_pet_window_to_mode,
};
pub use bridge::{live_signal_from_session_status, monitor_agent_from_cli, navigation_result};
pub use alerts::{
    AlertPreferences, WhatsAppProvider, alerts_prefs_path, format_agent_alert_message,
    send_whatsapp_callmebot,
};
pub use model::{CliAgentMonitorEvent, CliAgentMonitorModel, PetAlert};
pub use panel::CliAgentMonitorPanel;
pub use pet::{PetController, PetMode, PetPreferences, PetPosition, pet_prefs_path};
pub use panel_view::{
    CliAgentMonitorPanelAction, CliAgentMonitorPanelEvent, CliAgentMonitorPanelView,
};
pub use product::{
    ActivationOutcome, parse_terminal_view_id, product_activate_session, product_mark_reviewed,
};
pub use session::{LiveSessionSignal, MonitorSession, NavigationResult, map_live_signal};
pub use state::MonitorState;
pub use sound::{SessionAlertCue, play_cue, play_for_notification};
pub use store::{MonitorStore, NotificationKind, NotificationRequest};
pub use ui::{compact_indicator_from_store, compact_indicator_text, panel_row_from_session};

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
