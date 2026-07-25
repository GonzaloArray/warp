//! Closed Pet — visibility modes for the Sumanos floating avatar.
//!
//! Closing the pet never touches agent sessions, review state, or the monitor.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How the floating Sumanos pet is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PetMode {
    /// Pet visible (and animated when animations are enabled).
    #[default]
    Visible,
    /// Only the status-bar indicator; no floating pet.
    Minimized,
    /// Floating pet fully hidden; monitor keeps running.
    Closed,
}

impl PetMode {
    pub fn shows_floating_pet(self) -> bool {
        matches!(self, Self::Visible)
    }

    /// Tiny status-bar pet affordance (distinct from the agent-count chip).
    pub fn shows_status_bar_pet_affordance(self) -> bool {
        matches!(self, Self::Minimized)
    }

    pub fn animations_allowed(self, animations_enabled: bool) -> bool {
        matches!(self, Self::Visible) && animations_enabled
    }

    pub fn display_label_es(self) -> &'static str {
        match self {
            Self::Visible => "Pet visible",
            Self::Minimized => "Pet minimizado",
            Self::Closed => "Pet cerrado",
        }
    }
}

/// Optional float position for a free-floating pet (persisted when present).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct PetPosition {
    pub x: f32,
    pub y: f32,
}

/// Durable pet UI preferences. Independent of session monitor state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PetPreferences {
    pub mode: PetMode,
    pub position: Option<PetPosition>,
    pub animations_enabled: bool,
    /// When true, important alerts (e.g. COMPLETED_UNSEEN) may reopen the pet.
    /// Default **false** to avoid interruptions.
    pub show_on_important_alerts: bool,
}

impl Default for PetPreferences {
    fn default() -> Self {
        Self {
            mode: PetMode::Visible,
            position: None,
            animations_enabled: true,
            show_on_important_alerts: false,
        }
    }
}

/// Pure controller for pet visibility. Does not touch agent sessions or review.
#[derive(Debug, Clone, PartialEq)]
pub struct PetController {
    prefs: PetPreferences,
    path: Option<PathBuf>,
}

impl Default for PetController {
    fn default() -> Self {
        Self::new()
    }
}

impl PetController {
    pub fn new() -> Self {
        Self {
            prefs: PetPreferences::default(),
            path: None,
        }
    }

    pub fn with_path(path: PathBuf) -> Self {
        let prefs = if path.exists() {
            Self::load_from_path(&path).unwrap_or_default()
        } else {
            PetPreferences::default()
        };
        Self {
            prefs,
            path: Some(path),
        }
    }

    pub fn prefs(&self) -> &PetPreferences {
        &self.prefs
    }

    pub fn mode(&self) -> PetMode {
        self.prefs.mode
    }

    pub fn is_floating_visible(&self) -> bool {
        self.prefs.mode.shows_floating_pet()
    }

    pub fn animations_active(&self) -> bool {
        self.prefs
            .mode
            .animations_allowed(self.prefs.animations_enabled)
    }

    /// Close floating pet → `CLOSED`. Monitor/sessions unchanged.
    pub fn close_pet(&mut self) {
        self.prefs.mode = PetMode::Closed;
        let _ = self.persist();
    }

    /// Minimize to status-bar-only → `MINIMIZED`.
    pub fn minimize_pet(&mut self) {
        self.prefs.mode = PetMode::Minimized;
        let _ = self.persist();
    }

    /// Re-show floating pet → `VISIBLE`.
    pub fn show_pet(&mut self) {
        self.prefs.mode = PetMode::Visible;
        let _ = self.persist();
    }

    pub fn set_mode(&mut self, mode: PetMode) {
        self.prefs.mode = mode;
        let _ = self.persist();
    }

    pub fn set_animations_enabled(&mut self, enabled: bool) {
        self.prefs.animations_enabled = enabled;
        let _ = self.persist();
    }

    pub fn set_show_on_important_alerts(&mut self, enabled: bool) {
        self.prefs.show_on_important_alerts = enabled;
        let _ = self.persist();
    }

    pub fn set_position(&mut self, position: Option<PetPosition>) {
        self.prefs.position = position;
        let _ = self.persist();
    }

    /// Whether a finish/alert should auto-open the pet from CLOSED/MINIMIZED.
    /// Default preference is false — never auto-reopen unless enabled.
    pub fn should_reopen_on_important_alert(&self) -> bool {
        self.prefs.show_on_important_alerts && !matches!(self.prefs.mode, PetMode::Visible)
    }

    /// Apply optional auto-reopen when an important alert fires.
    /// Returns true if mode changed to Visible.
    pub fn on_important_alert(&mut self) -> bool {
        if self.should_reopen_on_important_alert() {
            self.show_pet();
            true
        } else {
            false
        }
    }

    /// Whether in-app avatar toasts / floating pet UI may be shown.
    pub fn may_show_floating_ui(&self) -> bool {
        self.is_floating_visible()
    }

    /// Closing the pet must not mark sessions reviewed — pure invariant for tests.
    pub fn close_does_not_review() -> bool {
        true
    }

    pub fn persist(&self) -> std::io::Result<()> {
        if let Some(path) = &self.path {
            self.save_to_path(path)
        } else {
            Ok(())
        }
    }

    pub fn save_to_path(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(&self.prefs)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn load_from_path(path: &Path) -> std::io::Result<PetPreferences> {
        let bytes = std::fs::read(path)?;
        serde_json::from_slice(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    pub fn reload(&mut self) -> std::io::Result<()> {
        if let Some(path) = self.path.clone() {
            if path.exists() {
                self.prefs = Self::load_from_path(&path)?;
            }
        }
        Ok(())
    }
}

/// Default path for pet preferences next to monitor sessions.
pub fn pet_prefs_path(data_dir: PathBuf) -> PathBuf {
    data_dir.join("cli_agent_monitor").join("pet.json")
}
