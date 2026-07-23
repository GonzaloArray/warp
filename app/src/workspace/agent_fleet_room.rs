//! Fleet Room — multi-agent group chat domain (BLOOME-like surface).
//!
//! Pure domain: no WarpUI. Workspace owns execution (launch CLIs) from
//! [`FleetSendPlan`]. Topology/voice of subagents stay in other modules.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::workspace::agent_provider_hub::AgentProviderId;

/// Stable room id for the default local group.
pub(crate) const DEFAULT_FLEET_ROOM_ID: &str = "local-agents";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum FleetMemberStatus {
    Idle,
    Working,
    NeedsYou,
    Offline,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FleetMember {
    pub provider: AgentProviderId,
    /// e.g. "Claude Code #1"
    pub slot_label: String,
    pub status: FleetMemberStatus,
    /// CLI binary detected on PATH.
    pub ready: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum FleetMessageKind {
    User,
    System,
    Agent,
    Route,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum FleetAuthor {
    User,
    System,
    Provider(AgentProviderId),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FleetMessage {
    pub id: u64,
    pub author: FleetAuthor,
    pub kind: FleetMessageKind,
    pub body: String,
    pub created_ms: u64,
}

/// One CLI launch/route requested by a send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FleetRoute {
    pub provider: AgentProviderId,
    /// User text with @tokens stripped (prompt to deliver).
    pub prompt: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FleetSendPlan {
    pub routes: Vec<FleetRoute>,
    pub messages_appended: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FleetRoom {
    pub id: String,
    pub title: String,
    pub members: Vec<FleetMember>,
    pub messages: Vec<FleetMessage>,
    pub selected_provider: Option<AgentProviderId>,
    next_message_id: u64,
    welcome_seeded: bool,
}

impl Default for FleetRoom {
    fn default() -> Self {
        Self::new_local()
    }
}

impl FleetRoom {
    pub(crate) fn new_local() -> Self {
        Self {
            id: DEFAULT_FLEET_ROOM_ID.into(),
            title: "Local Agents".into(),
            members: Vec::new(),
            messages: Vec::new(),
            selected_provider: None,
            next_message_id: 1,
            welcome_seeded: false,
        }
    }

    /// Sync members from hub-enabled providers + PATH readiness.
    pub(crate) fn sync_members(
        &mut self,
        enabled: impl IntoIterator<Item = AgentProviderId>,
        is_ready: impl Fn(AgentProviderId) -> bool,
    ) {
        let enabled: BTreeSet<_> = enabled.into_iter().collect();
        // Drop members no longer enabled.
        self.members.retain(|m| enabled.contains(&m.provider));
        for (i, provider) in enabled.into_iter().enumerate() {
            let ready = is_ready(provider);
            if let Some(existing) = self.members.iter_mut().find(|m| m.provider == provider) {
                existing.ready = ready;
                if existing.slot_label.is_empty() {
                    existing.slot_label = slot_label(provider, i + 1);
                }
            } else {
                self.members.push(FleetMember {
                    provider,
                    slot_label: slot_label(provider, i + 1),
                    status: if ready {
                        FleetMemberStatus::Idle
                    } else {
                        FleetMemberStatus::Offline
                    },
                    ready,
                });
            }
        }
        self.members
            .sort_by_key(|m| provider_sort_key(m.provider));
    }

    /// BLOOME-style intro lines once per room lifetime.
    pub(crate) fn seed_welcome_if_needed(&mut self, now_ms: u64) {
        if self.welcome_seeded || self.members.is_empty() {
            return;
        }
        self.welcome_seeded = true;
        self.push_message(
            FleetAuthor::System,
            FleetMessageKind::System,
            "Sala multiagent local. Usá @claude @codex @grok o @all para dirigir el mensaje."
                .into(),
            now_ms,
        );
        for member in self.members.clone() {
            let body = format!(
                "Hola 👋 soy {}. Escribime con @{} en este grupo y trabajamos juntos.",
                member.slot_label,
                mention_token(member.provider)
            );
            self.push_message(
                FleetAuthor::Provider(member.provider),
                FleetMessageKind::Agent,
                body,
                now_ms,
            );
        }
        self.push_message(
            FleetAuthor::System,
            FleetMessageKind::System,
            "Definí el objetivo del loop y mencioná a quién querés en la tarea.".into(),
            now_ms,
        );
    }

    pub(crate) fn select_member(&mut self, provider: Option<AgentProviderId>) {
        self.selected_provider = provider;
    }

    /// Parse draft, append user + route messages, return launch plan.
    pub(crate) fn send_user_message(&mut self, draft: &str, now_ms: u64) -> Option<FleetSendPlan> {
        let raw = draft.trim();
        if raw.is_empty() {
            return None;
        }
        let targets = resolve_targets(raw, &self.members);
        if targets.is_empty() {
            self.push_message(
                FleetAuthor::System,
                FleetMessageKind::System,
                "No hay agents listos para recibir el mensaje. Activá un provider o instalá su CLI."
                    .into(),
                now_ms,
            );
            return Some(FleetSendPlan {
                routes: Vec::new(),
                messages_appended: 1,
            });
        }
        let prompt = strip_mentions(raw);
        let prompt = if prompt.is_empty() {
            raw.to_string()
        } else {
            prompt
        };

        self.push_message(
            FleetAuthor::User,
            FleetMessageKind::User,
            raw.to_string(),
            now_ms,
        );

        let mut routes = Vec::new();
        let mut appended = 1usize;
        for provider in targets {
            let label = self
                .members
                .iter()
                .find(|m| m.provider == provider)
                .map(|m| m.slot_label.clone())
                .unwrap_or_else(|| provider.display_name().to_string());
            self.push_message(
                FleetAuthor::System,
                FleetMessageKind::Route,
                format!("→ Enviando a {label}…"),
                now_ms,
            );
            appended += 1;
            if let Some(m) = self.members.iter_mut().find(|m| m.provider == provider) {
                m.status = FleetMemberStatus::Working;
            }
            routes.push(FleetRoute {
                provider,
                prompt: prompt.clone(),
            });
        }
        Some(FleetSendPlan {
            routes,
            messages_appended: appended,
        })
    }

    fn push_message(
        &mut self,
        author: FleetAuthor,
        kind: FleetMessageKind,
        body: String,
        created_ms: u64,
    ) {
        let id = self.next_message_id;
        self.next_message_id = self.next_message_id.saturating_add(1);
        self.messages.push(FleetMessage {
            id,
            author,
            kind,
            body,
            created_ms,
        });
    }
}

fn slot_label(provider: AgentProviderId, n: usize) -> String {
    format!("{} #{n}", provider.display_name())
}

fn provider_sort_key(id: AgentProviderId) -> u8 {
    match id {
        AgentProviderId::Codex => 0,
        AgentProviderId::Claude => 1,
        AgentProviderId::Grok => 2,
        AgentProviderId::Gemini => 3,
        AgentProviderId::Hermes => 4,
        AgentProviderId::OpenCode => 5,
        AgentProviderId::Kimi => 6,
        AgentProviderId::MiniMax => 7,
        AgentProviderId::Cursor => 8,
        AgentProviderId::Copilot => 9,
    }
}

/// Canonical @token without leading @ (lowercase).
pub(crate) fn mention_token(provider: AgentProviderId) -> &'static str {
    match provider {
        AgentProviderId::Claude => "claude",
        AgentProviderId::Codex => "codex",
        AgentProviderId::Gemini => "gemini",
        AgentProviderId::Grok => "grok",
        AgentProviderId::Kimi => "kimi",
        AgentProviderId::MiniMax => "minimax",
        AgentProviderId::OpenCode => "opencode",
        AgentProviderId::Hermes => "hermes",
        AgentProviderId::Cursor => "cursor",
        AgentProviderId::Copilot => "copilot",
    }
}

pub(crate) fn provider_from_mention(token: &str) -> Option<AgentProviderId> {
    let t = token.trim().trim_start_matches('@').to_ascii_lowercase();
    match t.as_str() {
        "claude" | "claudecode" | "claude-code" => Some(AgentProviderId::Claude),
        "codex" | "openai" => Some(AgentProviderId::Codex),
        "gemini" | "geminicli" => Some(AgentProviderId::Gemini),
        "grok" | "xai" => Some(AgentProviderId::Grok),
        "kimi" | "moonshot" => Some(AgentProviderId::Kimi),
        "minimax" | "mini-max" => Some(AgentProviderId::MiniMax),
        "opencode" | "open-code" => Some(AgentProviderId::OpenCode),
        "hermes" => Some(AgentProviderId::Hermes),
        "cursor" | "cursor-agent" => Some(AgentProviderId::Cursor),
        "copilot" => Some(AgentProviderId::Copilot),
        _ => None,
    }
}

fn is_broadcast_token(token: &str) -> bool {
    matches!(
        token.trim().trim_start_matches('@').to_ascii_lowercase().as_str(),
        "all" | "todos" | "everyone" | "everyonee" | "fleet" | "grupo"
    )
}

/// Extract @tokens (without @).
pub(crate) fn parse_mentions(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.split_whitespace() {
        let piece = raw.trim_matches(|c: char| {
            matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | '(' | ')' | '"' | '\'' )
        });
        if let Some(rest) = piece.strip_prefix('@') {
            if !rest.is_empty() {
                out.push(rest.to_ascii_lowercase());
            }
        }
    }
    out
}

pub(crate) fn strip_mentions(text: &str) -> String {
    text.split_whitespace()
        .filter(|w| {
            let t = w.trim_matches(|c: char| {
                matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | '(' | ')' | '"' | '\'')
            });
            !t.starts_with('@')
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn resolve_targets(text: &str, members: &[FleetMember]) -> Vec<AgentProviderId> {
    let mentions = parse_mentions(text);
    let enabled: Vec<AgentProviderId> = members.iter().map(|m| m.provider).collect();
    let ready: Vec<AgentProviderId> = members
        .iter()
        .filter(|m| m.ready)
        .map(|m| m.provider)
        .collect();

    if mentions.is_empty() {
        return if !ready.is_empty() { ready } else { enabled };
    }

    let mut broadcast = false;
    let mut specific = BTreeSet::new();
    for token in &mentions {
        if is_broadcast_token(token) {
            broadcast = true;
        } else if let Some(p) = provider_from_mention(token) {
            if enabled.contains(&p) {
                specific.insert(p);
            }
        }
    }

    if broadcast {
        return if !ready.is_empty() { ready } else { enabled };
    }
    specific.into_iter().collect()
}

/// Shell line that starts the CLI with an optional initial prompt.
/// Interactive-friendly: prefer bare CLI when prompt empty; else quote-safe append.
pub(crate) fn launch_shell_for_route(provider: AgentProviderId, prompt: &str) -> String {
    let base = provider.launch_command();
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return base.to_string();
    }
    // Single-quoted shell string; strip quotes inside to avoid breakout.
    let safe: String = prompt.chars().filter(|c| *c != '\'' && *c != '\n').collect();
    let safe = if safe.chars().count() > 400 {
        safe.chars().take(400).collect::<String>()
    } else {
        safe
    };
    match provider {
        // Claude Code: trailing args often become the first user turn in interactive mode
        // depending on version; still better than only opening empty shell.
        AgentProviderId::Claude
        | AgentProviderId::Codex
        | AgentProviderId::Gemini
        | AgentProviderId::Grok
        | AgentProviderId::Hermes
        | AgentProviderId::OpenCode
        | AgentProviderId::Kimi
        | AgentProviderId::MiniMax
        | AgentProviderId::Cursor
        | AgentProviderId::Copilot => format!("{base} '{safe}'"),
    }
}

#[cfg(test)]
#[path = "agent_fleet_room_tests.rs"]
mod tests;
