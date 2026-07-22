//! Catalog of external agent providers managed from the vertical-tabs monitor.
//!
//! This is the connection surface for Codex / Claude / Grok / etc. It does not
//! run inference itself: Launch opens a terminal and starts the provider CLI.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use crate::terminal::CLIAgent;

const SCHEMA_VERSION: u32 = 1;

/// Product-level provider the user can connect and launch from the monitor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentProviderId {
    Claude,
    Codex,
    Gemini,
    Grok,
    Kimi,
    MiniMax,
    OpenCode,
    Hermes,
    Cursor,
    Copilot,
}

impl AgentProviderId {
    pub(crate) const ALL: [Self; 10] = [
        Self::Claude,
        Self::Codex,
        Self::Gemini,
        Self::Grok,
        Self::Kimi,
        Self::MiniMax,
        Self::OpenCode,
        Self::Hermes,
        Self::Cursor,
        Self::Copilot,
    ];

    /// Default-on for the hub. Others remain available but start disabled so
    /// the list stays scannable.
    pub(crate) const DEFAULT_ENABLED: [Self; 6] = [
        Self::Claude,
        Self::Codex,
        Self::Gemini,
        Self::Grok,
        Self::Kimi,
        Self::MiniMax,
    ];

    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini",
            Self::Grok => "Grok",
            Self::Kimi => "Kimi",
            Self::MiniMax => "MiniMax",
            Self::OpenCode => "OpenCode",
            Self::Hermes => "Hermes",
            Self::Cursor => "Cursor",
            Self::Copilot => "Copilot",
        }
    }

    /// Shell command launched in a new terminal. Prefer the official CLI name.
    pub(crate) fn launch_command(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Grok => "grok",
            Self::Kimi => "kimi",
            Self::MiniMax => "minimax",
            Self::OpenCode => "opencode",
            Self::Hermes => "hermes",
            Self::Cursor => "agent",
            Self::Copilot => "copilot",
        }
    }

    /// Binaries accepted as “installed” for detection.
    pub(crate) fn detect_commands(self) -> &'static [&'static str] {
        match self {
            Self::Claude => &["claude"],
            Self::Codex => &["codex"],
            Self::Gemini => &["gemini"],
            Self::Grok => &["grok", "xai"],
            Self::Kimi => &["kimi", "moonshot"],
            Self::MiniMax => &["minimax", "mini-max"],
            Self::OpenCode => &["opencode"],
            Self::Hermes => &["hermes"],
            Self::Cursor => &["agent", "cursor-agent"],
            Self::Copilot => &["copilot"],
        }
    }

    pub(crate) fn from_cli(agent: CLIAgent) -> Option<Self> {
        match agent {
            CLIAgent::Claude => Some(Self::Claude),
            CLIAgent::Codex => Some(Self::Codex),
            CLIAgent::Gemini => Some(Self::Gemini),
            CLIAgent::Grok => Some(Self::Grok),
            CLIAgent::Kimi => Some(Self::Kimi),
            CLIAgent::MiniMax => Some(Self::MiniMax),
            CLIAgent::OpenCode => Some(Self::OpenCode),
            CLIAgent::Hermes => Some(Self::Hermes),
            CLIAgent::CursorCli => Some(Self::Cursor),
            CLIAgent::Copilot => Some(Self::Copilot),
            _ => None,
        }
    }

    /// Brand icon for menus when available; falls back to a generic agent icon.
    pub(crate) fn menu_icon(self) -> warp_core::ui::icons::Icon {
        use warp_core::ui::icons::Icon;
        match self {
            Self::Claude => Icon::ClaudeLogo,
            Self::Codex => Icon::OpenAILogo,
            Self::Gemini => Icon::GeminiLogo,
            Self::OpenCode => Icon::OpenCodeLogo,
            Self::Cursor => Icon::CursorLogo,
            Self::Copilot => Icon::CopilotLogo,
            Self::Grok | Self::Kimi | Self::MiniMax | Self::Hermes => Icon::AiAssistant,
        }
    }

    /// Whether this provider exposes a first-class “continue last session” path.
    pub(crate) fn supports_resume(self) -> bool {
        matches!(
            self,
            Self::Claude | Self::Codex | Self::OpenCode | Self::Gemini
        )
    }

    /// Full shell line for Launch (New session vs Resume last).
    ///
    /// These are intentional product defaults — not a generic “type the binary
    /// name” dump. Auth/models still live with the CLI.
    pub(crate) fn launch_shell_line(self, mode: AgentProviderLaunchMode) -> String {
        match (self, mode) {
            (Self::Claude, AgentProviderLaunchMode::NewSession) => "claude".to_string(),
            (Self::Claude, AgentProviderLaunchMode::ResumeLast) => {
                "claude --continue".to_string()
            }
            (Self::Codex, AgentProviderLaunchMode::NewSession) => "codex".to_string(),
            (Self::Codex, AgentProviderLaunchMode::ResumeLast) => {
                "codex resume --last".to_string()
            }
            (Self::Gemini, AgentProviderLaunchMode::NewSession) => "gemini".to_string(),
            (Self::Gemini, AgentProviderLaunchMode::ResumeLast) => {
                "gemini --resume latest".to_string()
            }
            (Self::OpenCode, AgentProviderLaunchMode::NewSession) => "opencode".to_string(),
            (Self::OpenCode, AgentProviderLaunchMode::ResumeLast) => "opencode".to_string(),
            (Self::Grok, _) => "grok".to_string(),
            (Self::Kimi, _) => "kimi".to_string(),
            (Self::MiniMax, _) => "minimax".to_string(),
            (Self::Hermes, _) => "hermes".to_string(),
            (Self::Cursor, _) => "agent".to_string(),
            (Self::Copilot, _) => "copilot".to_string(),
        }
    }

    /// Short action label for menus (Spanish product copy for this fork).
    pub(crate) fn launch_action_label(self, mode: AgentProviderLaunchMode) -> &'static str {
        match mode {
            AgentProviderLaunchMode::NewSession => "Nueva sesión",
            AgentProviderLaunchMode::ResumeLast => match self {
                Self::Claude => "Continuar última",
                Self::Codex => "Resume última",
                _ => "Continuar",
            },
        }
    }

    /// Tooltip explaining what Launch will do (more than “type codex”).
    pub(crate) fn launch_tooltip(self, mode: AgentProviderLaunchMode) -> String {
        use crate::workspace::agent_presentation::AgentUiProfile;
        use crate::workspace::agent_tabs_projection::ExternalProvider;

        let cmd = self.launch_shell_line(mode);
        let blurb = ExternalProvider::from_cli(self.to_cli_agent())
            .map(AgentUiProfile::for_provider)
            .map(|p| p.capabilities_blurb)
            .unwrap_or("CLI externo en el monitor de agentes");
        match mode {
            AgentProviderLaunchMode::NewSession => format!(
                "Abre un terminal, lanza `{cmd}` y lo muestra en el rail. {blurb}."
            ),
            AgentProviderLaunchMode::ResumeLast => format!(
                "Retoma la última sesión de {} con `{cmd}`. {blurb}.",
                self.display_name()
            ),
        }
    }

    fn to_cli_agent(self) -> crate::terminal::CLIAgent {
        match self {
            Self::Claude => crate::terminal::CLIAgent::Claude,
            Self::Codex => crate::terminal::CLIAgent::Codex,
            Self::Gemini => crate::terminal::CLIAgent::Gemini,
            Self::Grok => crate::terminal::CLIAgent::Grok,
            Self::Kimi => crate::terminal::CLIAgent::Kimi,
            Self::MiniMax => crate::terminal::CLIAgent::MiniMax,
            Self::OpenCode => crate::terminal::CLIAgent::OpenCode,
            Self::Hermes => crate::terminal::CLIAgent::Hermes,
            Self::Cursor => crate::terminal::CLIAgent::CursorCli,
            Self::Copilot => crate::terminal::CLIAgent::Copilot,
        }
    }
}

/// How the + menu / Settings Launch button starts a provider CLI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentProviderLaunchMode {
    /// Fresh interactive session for this provider.
    #[default]
    NewSession,
    /// Continue / resume the most recent session when the CLI supports it.
    ResumeLast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProviderInstallState {
    Ready,
    Missing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProviderHubRow {
    pub id: AgentProviderId,
    pub enabled: bool,
    pub install: ProviderInstallState,
    pub active_sessions: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct AgentProviderHubPrefs {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    /// Providers the user wants available in the hub launch list.
    #[serde(default)]
    pub enabled: BTreeSet<AgentProviderId>,
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Default for AgentProviderHubPrefs {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            enabled: AgentProviderId::DEFAULT_ENABLED.into_iter().collect(),
        }
    }
}

impl AgentProviderHubPrefs {
    pub(crate) fn load_default() -> Self {
        load(&default_prefs_path())
    }

    pub(crate) fn save_default(&self) -> std::io::Result<()> {
        save(&default_prefs_path(), self)
    }

    pub(crate) fn is_enabled(&self, id: AgentProviderId) -> bool {
        self.enabled.contains(&id)
    }

    pub(crate) fn set_enabled(&mut self, id: AgentProviderId, enabled: bool) {
        if enabled {
            self.enabled.insert(id);
        } else {
            self.enabled.remove(&id);
        }
    }
}

pub(crate) fn default_prefs_path() -> PathBuf {
    warp_core::paths::config_local_dir().join("agent-provider-hub.json")
}

pub(crate) fn load(path: &Path) -> AgentProviderHubPrefs {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .filter(|prefs: &AgentProviderHubPrefs| prefs.schema_version <= SCHEMA_VERSION)
        .unwrap_or_default()
}

pub(crate) fn save(path: &Path, prefs: &AgentProviderHubPrefs) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_vec_pretty(prefs).expect("AgentProviderHubPrefs is serializable");
    fs::write(path, data)
}

/// Snapshot rows for the Providers section. `active_by_provider` is built from
/// live CLI sessions so the hub reflects what is already running.
pub(crate) fn provider_hub_rows(
    prefs: &AgentProviderHubPrefs,
    active_by_provider: &HashMap<AgentProviderId, usize>,
    on_path: impl Fn(&str) -> bool,
) -> Vec<ProviderHubRow> {
    AgentProviderId::ALL
        .into_iter()
        .map(|id| {
            let install = if id.detect_commands().iter().any(|cmd| on_path(cmd)) {
                ProviderInstallState::Ready
            } else {
                ProviderInstallState::Missing
            };
            ProviderHubRow {
                id,
                enabled: prefs.is_enabled(id),
                install,
                active_sessions: active_by_provider.get(&id).copied().unwrap_or(0),
            }
        })
        .collect()
}

/// Best-effort PATH lookup. Avoids spawning `which` so the rail can refresh
/// without blocking on a subprocess.
pub(crate) fn command_on_path(command: &str) -> bool {
    if command.is_empty() || command.contains('/') || command.contains('\\') {
        return false;
    }
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return true;
        }
        #[cfg(windows)]
        {
            let with_exe = dir.join(format!("{command}.exe"));
            if with_exe.is_file() {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
#[path = "agent_provider_hub_tests.rs"]
mod tests;
