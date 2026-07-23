//! Warp-owned screen-evidence classification for external agents.
//!
//! Pure domain: given a short evidence snapshot (OSC title + recent bottom
//! lines), classify glanceable status. **Blocked is strict** — only known
//! approval / permission / question UI. Conceptual inspiration only (no Herdr
//! manifests or code).

use crate::workspace::agent_tabs_projection::{AgentTabStatus, ExternalProvider};

/// Evidence gathered from a live agent surface (not the scrolled viewport).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ScreenEvidence {
    /// Latest OSC 0/2 title when available.
    pub title: Option<String>,
    /// Last non-empty lines from the live bottom of the buffer (newest last).
    pub bottom_lines: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EvidenceAuthority {
    /// Explicit permission / blocked UI.
    BlockedUi,
    /// Spinner / working chrome.
    WorkingUi,
    /// Idle / ready prompt without blockers.
    IdleUi,
    /// No confident match.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScreenClassification {
    pub status: AgentTabStatus,
    pub authority: EvidenceAuthority,
    pub matched_rule: &'static str,
}

/// Classify status from screen evidence for a known provider.
///
/// Returns `None` when evidence is empty or no rule matches (caller keeps
/// process/heuristic status).
pub(crate) fn classify_screen_evidence(
    provider: ExternalProvider,
    evidence: &ScreenEvidence,
) -> Option<ScreenClassification> {
    let title = evidence.title.as_deref().unwrap_or("").trim();
    let bottom = evidence
        .bottom_lines
        .iter()
        .map(|l| l.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let haystack = format!("{title}\n{bottom}").to_lowercase();
    if haystack.trim().is_empty() {
        return None;
    }

    // Strict blocked first (all providers that show approval UI).
    if let Some(rule) = match_blocked(&haystack, provider) {
        return Some(ScreenClassification {
            status: AgentTabStatus::Blocked,
            authority: EvidenceAuthority::BlockedUi,
            matched_rule: rule,
        });
    }

    if let Some(rule) = match_working(&haystack, provider) {
        return Some(ScreenClassification {
            status: AgentTabStatus::Working,
            authority: EvidenceAuthority::WorkingUi,
            matched_rule: rule,
        });
    }

    if let Some(rule) = match_idle(&haystack, provider) {
        return Some(ScreenClassification {
            status: AgentTabStatus::Completed,
            authority: EvidenceAuthority::IdleUi,
            matched_rule: rule,
        });
    }

    None
}

/// Merge screen classification into an existing status without downgrading
/// Failed, and without inventing Blocked from weak signals (already enforced
/// in `classify_screen_evidence`).
pub(crate) fn merge_status_with_evidence(
    current: AgentTabStatus,
    classified: Option<ScreenClassification>,
) -> AgentTabStatus {
    let Some(c) = classified else {
        return current;
    };
    match (current, c.status) {
        // Never hide a hard failure behind screen idle/working.
        (AgentTabStatus::Failed, _) => AgentTabStatus::Failed,
        // Screen blocked upgrades waiting/working/unavailable.
        (
            AgentTabStatus::Working
            | AgentTabStatus::Waiting
            | AgentTabStatus::Unavailable
            | AgentTabStatus::Completed,
            AgentTabStatus::Blocked,
        ) => AgentTabStatus::Blocked,
        // Working screen keeps completed from flashing over active work.
        (AgentTabStatus::Completed, AgentTabStatus::Working) => AgentTabStatus::Working,
        // Trust stronger blocked/failed already present.
        (AgentTabStatus::Blocked, _) => AgentTabStatus::Blocked,
        (_, next) => next,
    }
}

fn match_blocked(hay: &str, provider: ExternalProvider) -> Option<&'static str> {
    // Shared strict blockers (approval / human question).
    const SHARED: &[(&str, &str)] = &[
        ("action required", "title_action_required"),
        ("allow command?", "allow_command"),
        ("do you want to", "do_you_want"),
        ("would you like to", "would_you_like"),
        ("press enter to confirm", "press_enter_confirm"),
        ("enter to submit answer", "submit_answer"),
        ("permission", "permission_word"),
        ("[y/n]", "yn_prompt"),
        ("yes (y)", "yes_y"),
        ("askuserquestion", "ask_user_question"),
    ];
    for (needle, rule) in SHARED {
        if hay.contains(needle) {
            // "permission" alone is too broad unless paired with want/allow.
            if *rule == "permission_word"
                && !(hay.contains("wants to")
                    || hay.contains("allow")
                    || hay.contains("request")
                    || hay.contains("denied"))
            {
                continue;
            }
            return Some(rule);
        }
    }
    match provider {
        ExternalProvider::Codex => {
            if hay.contains("esc to cancel") && hay.contains("enter") {
                return Some("codex_confirm_cancel");
            }
        }
        ExternalProvider::Claude => {
            if hay.contains("bypass permissions") || hay.contains("tool use") && hay.contains("y/n")
            {
                return Some("claude_permission_gate");
            }
        }
        _ => {}
    }
    None
}

fn match_working(hay: &str, provider: ExternalProvider) -> Option<&'static str> {
    // Spinners / activity chrome (ASCII approximations + keywords).
    if hay.contains("esc to interrupt") || hay.contains("working (") {
        return Some("working_interrupt_hint");
    }
    // Braille spinner glyphs often appear in titles.
    const SPINNERS: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    for s in SPINNERS {
        if hay.contains(s) {
            return Some("spinner_glyph");
        }
    }
    if matches!(
        provider,
        ExternalProvider::Codex | ExternalProvider::Claude
    ) && (hay.contains("thinking") || hay.contains("running tool") || hay.contains("tool call"))
    {
        return Some("agent_activity_keyword");
    }
    None
}

fn match_idle(hay: &str, provider: ExternalProvider) -> Option<&'static str> {
    match provider {
        ExternalProvider::Codex => {
            if hay.contains("ctrl+c to exit") || hay.contains("send a message") {
                return Some("codex_ready_prompt");
            }
        }
        ExternalProvider::Claude => {
            if hay.contains("ready") && hay.contains("claude") {
                return Some("claude_ready");
            }
        }
        _ => {}
    }
    None
}

/// Build evidence from optional title + free-form recent text lines.
pub(crate) fn evidence_from_parts(
    title: Option<&str>,
    recent_lines: impl IntoIterator<Item = impl AsRef<str>>,
) -> ScreenEvidence {
    ScreenEvidence {
        title: title
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        bottom_lines: recent_lines
            .into_iter()
            .map(|l| l.as_ref().trim().to_owned())
            .filter(|l| !l.is_empty())
            .collect(),
    }
}

#[cfg(test)]
#[path = "agent_screen_evidence_tests.rs"]
mod tests;
