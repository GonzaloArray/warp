//! Session alert sounds for the CLI agent monitor (human-gate, completed, error).
//!
//! Designed so each notification is sonically distinct and reliable even when
//! Notification Center is muted: we play a system sound file (macOS `afplay`)
//! and optionally fall back to [`crate::terminal::AudibleBell`].

use std::collections::HashMap;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::store::NotificationKind;

/// Minimum gap between two plays of the same session+kind (anti-spam).
pub const SESSION_SOUND_COOLDOWN: Duration = Duration::from_millis(1800);

/// Semantic cue for a monitor alert. Maps 1:1 to a high-quality system sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionAlertCue {
    /// Agent needs human input (human-gate).
    HumanGate,
    /// Agent finished successfully — review pending.
    Completed,
    /// Agent failed / error state.
    Error,
}

impl SessionAlertCue {
    pub fn from_notification_kind(kind: NotificationKind) -> Self {
        match kind {
            NotificationKind::HumanGate => Self::HumanGate,
            NotificationKind::Completed => Self::Completed,
        }
    }

    /// Human-readable name for logs / prefs.
    pub fn label(self) -> &'static str {
        match self {
            Self::HumanGate => "human-gate",
            Self::Completed => "completed",
            Self::Error => "error",
        }
    }

    /// macOS system sound path (present on all modern macOS installs).
    ///
    /// - **Ping**: sharp attention (gate)
    /// - **Glass**: clear “done” chime
    /// - **Basso**: low failure tone
    pub fn macos_sound_path(self) -> &'static str {
        match self {
            Self::HumanGate => "/System/Library/Sounds/Ping.aiff",
            Self::Completed => "/System/Library/Sounds/Glass.aiff",
            Self::Error => "/System/Library/Sounds/Basso.aiff",
        }
    }

    /// How many times to play (human-gate = double ping so it cuts through).
    pub fn play_count(self) -> u8 {
        match self {
            Self::HumanGate => 2,
            Self::Completed | Self::Error => 1,
        }
    }

    /// Gap between repeats of the same cue.
    pub fn repeat_gap(self) -> Duration {
        match self {
            Self::HumanGate => Duration::from_millis(220),
            Self::Completed | Self::Error => Duration::ZERO,
        }
    }
}

/// Pure: should we play given last play time and now?
pub fn should_play_with_cooldown(
    last: Option<Instant>,
    now: Instant,
    cooldown: Duration,
) -> bool {
    match last {
        None => true,
        Some(t) => now.duration_since(t) >= cooldown,
    }
}

/// Cooldown key: session + cue so human-gate and completed can both fire in one session.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CooldownKey {
    session_id: String,
    cue: SessionAlertCue,
}

static COOLDOWNS: OnceLock<Mutex<HashMap<CooldownKey, Instant>>> = OnceLock::new();

fn cooldowns() -> &'static Mutex<HashMap<CooldownKey, Instant>> {
    COOLDOWNS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Play the alert sound for this notification kind + session (non-blocking).
/// Returns whether a play was scheduled (false if cooldown suppressed).
pub fn play_for_notification(kind: NotificationKind, session_id: &str) -> bool {
    let cue = SessionAlertCue::from_notification_kind(kind);
    play_cue(cue, session_id)
}

/// Play a named cue with per-session cooldown. Fire-and-forget thread.
pub fn play_cue(cue: SessionAlertCue, session_id: &str) -> bool {
    let now = Instant::now();
    let key = CooldownKey {
        session_id: session_id.to_owned(),
        cue,
    };
    {
        let mut map = cooldowns().lock().unwrap_or_else(|e| e.into_inner());
        if !should_play_with_cooldown(map.get(&key).copied(), now, SESSION_SOUND_COOLDOWN) {
            log::debug!(
                "CLI monitor sound suppressed (cooldown) cue={} session={}",
                cue.label(),
                session_id
            );
            return false;
        }
        map.insert(key, now);
        // Bound map growth
        if map.len() > 256 {
            map.retain(|_, t| now.duration_since(*t) < Duration::from_secs(120));
        }
    }

    let count = cue.play_count();
    let gap = cue.repeat_gap();
    let path = cue.macos_sound_path();

    std::thread::Builder::new()
        .name("cli-monitor-sound".into())
        .spawn(move || {
            for i in 0..count {
                if i > 0 && !gap.is_zero() {
                    std::thread::sleep(gap);
                }
                if !play_sound_file(path) {
                    // Last-resort platform beep once.
                    let _ = fallback_beep();
                    break;
                }
            }
        })
        .ok();
    true
}

fn play_sound_file(path: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        Command::new("afplay")
            .arg(path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(target_os = "linux")]
    {
        // Prefer paplay/pw-play if available; else false → beep fallback.
        for bin in ["pw-play", "paplay", "aplay"] {
            if Command::new(bin)
                .arg(path)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
            {
                return true;
            }
        }
        false
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = path;
        false
    }
}

fn fallback_beep() -> bool {
    #[cfg(target_os = "macos")]
    {
        Command::new("osascript")
            .args(["-e", "beep"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        // ASCII BEL to controlling terminal (best-effort).
        eprint!("\x07");
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_mapping_from_notification_kind() {
        assert_eq!(
            SessionAlertCue::from_notification_kind(NotificationKind::HumanGate),
            SessionAlertCue::HumanGate
        );
        assert_eq!(
            SessionAlertCue::from_notification_kind(NotificationKind::Completed),
            SessionAlertCue::Completed
        );
    }

    #[test]
    fn human_gate_is_double_ping_completed_is_single_glass() {
        assert_eq!(SessionAlertCue::HumanGate.play_count(), 2);
        assert_eq!(SessionAlertCue::Completed.play_count(), 1);
        assert!(SessionAlertCue::HumanGate
            .macos_sound_path()
            .contains("Ping"));
        assert!(SessionAlertCue::Completed
            .macos_sound_path()
            .contains("Glass"));
        assert!(SessionAlertCue::Error.macos_sound_path().contains("Basso"));
    }

    #[test]
    fn cooldown_blocks_immediate_replay() {
        let t0 = Instant::now();
        assert!(should_play_with_cooldown(None, t0, SESSION_SOUND_COOLDOWN));
        assert!(!should_play_with_cooldown(
            Some(t0),
            t0 + Duration::from_millis(100),
            SESSION_SOUND_COOLDOWN
        ));
        assert!(should_play_with_cooldown(
            Some(t0),
            t0 + SESSION_SOUND_COOLDOWN + Duration::from_millis(1),
            SESSION_SOUND_COOLDOWN
        ));
    }

    #[test]
    fn cues_are_distinct_paths() {
        let paths = [
            SessionAlertCue::HumanGate.macos_sound_path(),
            SessionAlertCue::Completed.macos_sound_path(),
            SessionAlertCue::Error.macos_sound_path(),
        ];
        assert_eq!(paths.iter().collect::<std::collections::HashSet<_>>().len(), 3);
    }
}
