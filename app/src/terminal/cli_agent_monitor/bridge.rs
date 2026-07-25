//! Bridge from Warp `CLIAgent` / session status into the monitor domain.

use crate::terminal::CLIAgent;
use crate::terminal::cli_agent_sessions::CLIAgentSessionStatus;

use super::adapters::MonitorAgentKind;
use super::session::LiveSessionSignal;

pub fn monitor_agent_from_cli(agent: CLIAgent) -> Option<MonitorAgentKind> {
    match agent {
        CLIAgent::Claude => Some(MonitorAgentKind::Claude),
        CLIAgent::Codex => Some(MonitorAgentKind::Codex),
        CLIAgent::Grok => Some(MonitorAgentKind::Grok),
        // Other first-party CLI agents are not in the ADHD monitor v1 scope
        // but remain trackable as Other for visibility if desired later.
        CLIAgent::Gemini => Some(MonitorAgentKind::Other("Gemini".into())),
        CLIAgent::Amp => Some(MonitorAgentKind::Other("Amp".into())),
        CLIAgent::Droid => Some(MonitorAgentKind::Other("Droid".into())),
        CLIAgent::OpenCode => Some(MonitorAgentKind::Other("OpenCode".into())),
        CLIAgent::Copilot => Some(MonitorAgentKind::Other("Copilot".into())),
        CLIAgent::Pi => Some(MonitorAgentKind::Other("Pi".into())),
        CLIAgent::OhMyPi => Some(MonitorAgentKind::Other("oh-my-pi".into())),
        CLIAgent::Auggie => Some(MonitorAgentKind::Other("Auggie".into())),
        CLIAgent::CursorCli => Some(MonitorAgentKind::Other("Cursor".into())),
        CLIAgent::Goose => Some(MonitorAgentKind::Other("Goose".into())),
        CLIAgent::Hermes => Some(MonitorAgentKind::Other("Hermes".into())),
        CLIAgent::Vibe => Some(MonitorAgentKind::Other("Mistral Vibe".into())),
        CLIAgent::Antigravity => Some(MonitorAgentKind::Other("Antigravity".into())),
        CLIAgent::Unknown => None,
    }
}

pub fn live_signal_from_session_status(status: &CLIAgentSessionStatus) -> LiveSessionSignal {
    match status {
        CLIAgentSessionStatus::InProgress => LiveSessionSignal::InProgress,
        CLIAgentSessionStatus::Blocked { .. } => LiveSessionSignal::Blocked,
        CLIAgentSessionStatus::Success => LiveSessionSignal::Success,
        CLIAgentSessionStatus::Failed { .. } => LiveSessionSignal::Failed,
    }
}

/// Decide navigation result for a known terminal existence + focus success flag.
pub fn navigation_result(terminal_exists: bool, focus_succeeded: bool) -> super::NavigationResult {
    if !terminal_exists {
        super::NavigationResult::MissingTerminal
    } else if focus_succeeded {
        super::NavigationResult::Focused
    } else {
        super::NavigationResult::Failed
    }
}
