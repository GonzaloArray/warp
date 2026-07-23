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

    /// Short system intro once per room lifetime (no spam of fake agent hellos).
    pub(crate) fn seed_welcome_if_needed(&mut self, now_ms: u64) {
        if self.welcome_seeded || self.members.is_empty() {
            return;
        }
        self.welcome_seeded = true;
        let roster = self
            .members
            .iter()
            .map(|m| format!("@{}", mention_token(m.provider)))
            .collect::<Vec<_>>()
            .join(" ");
        self.push_message(
            FleetAuthor::System,
            FleetMessageKind::System,
            format!(
                "Local Agents = mesa de control de CLIs (Claude, Codex, Grok…), no un chat de IA.\n\
                 • Escribí @claude / @codex / @all + mensaje y Enviar → abre 1 tab por agent (CLI real).\n\
                 • Sin @ no se abre nada: solo se guarda el mensaje acá.\n\
                 • «Abrir CLI» en un member = solo ese terminal, vacío.\n\
                 Disponibles: {roster}"
            ),
            now_ms,
        );
    }

    pub(crate) fn select_member(&mut self, provider: Option<AgentProviderId>) {
        self.selected_provider = provider;
    }

    /// Parse draft, append user + route messages, return launch plan.
    ///
    /// Requires explicit `@mention` (or `@all`). Without mention we only keep
    /// the note in the thread — never auto-open every CLI (that froze Warp and
    /// confused users with N tabs full of the same text).
    pub(crate) fn send_user_message(&mut self, draft: &str, now_ms: u64) -> Option<FleetSendPlan> {
        let raw = draft.trim();
        if raw.is_empty() {
            return None;
        }

        self.push_message(
            FleetAuthor::User,
            FleetMessageKind::User,
            raw.to_string(),
            now_ms,
        );

        let mentions = parse_mentions(raw);
        if mentions.is_empty() {
            // Prefer selected member as implicit single target.
            if let Some(selected) = self.selected_provider {
                if self.members.iter().any(|m| m.provider == selected && m.ready) {
                    return Some(self.plan_routes(&[selected], now_ms, 1));
                }
            }
            self.push_message(
                FleetAuthor::System,
                FleetMessageKind::System,
                "Mensaje guardado en el hilo. Para abrir un CLI usá @claude, @codex, @all, \
                 o seleccioná un member y reenviá."
                    .into(),
                now_ms,
            );
            return Some(FleetSendPlan {
                routes: Vec::new(),
                messages_appended: 2,
            });
        }

        let targets = resolve_targets(raw, &self.members);
        if targets.is_empty() {
            self.push_message(
                FleetAuthor::System,
                FleetMessageKind::System,
                "No hay agents listos para ese @. Instalá el CLI o activá el provider en Ajustes."
                    .into(),
                now_ms,
            );
            return Some(FleetSendPlan {
                routes: Vec::new(),
                messages_appended: 2,
            });
        }

        // Cap fan-out: @all opening 6+ tabs freezes the UI.
        const MAX_PARALLEL_LAUNCHES: usize = 2;
        let total = targets.len();
        let capped: Vec<_> = targets.into_iter().take(MAX_PARALLEL_LAUNCHES).collect();
        let mut plan = self.plan_routes(&capped, now_ms, 1);
        if total > MAX_PARALLEL_LAUNCHES {
            self.push_message(
                FleetAuthor::System,
                FleetMessageKind::System,
                format!(
                    "Abrí solo {MAX_PARALLEL_LAUNCHES} terminals a la vez para no frezar Warp. \
                     Volvé a mandar @all para los siguientes."
                ),
                now_ms,
            );
            plan.messages_appended += 1;
        }
        Some(plan)
    }

    fn plan_routes(
        &mut self,
        targets: &[AgentProviderId],
        now_ms: u64,
        messages_already: usize,
    ) -> FleetSendPlan {
        let mut routes = Vec::new();
        let mut appended = messages_already;
        for provider in targets {
            let label = self
                .members
                .iter()
                .find(|m| m.provider == *provider)
                .map(|m| m.slot_label.clone())
                .unwrap_or_else(|| provider.display_name().to_string());
            self.push_message(
                FleetAuthor::System,
                FleetMessageKind::Route,
                format!(
                    "→ Abriendo terminal de {label}. El mensaje del hilo no se pega al shell \
                     (escribilo vos en el CLI si hace falta)."
                ),
                now_ms,
            );
            appended += 1;
            if let Some(m) = self.members.iter_mut().find(|m| m.provider == *provider) {
                m.status = FleetMemberStatus::Working;
            }
            // Bare CLI only — no argv dump of the fleet message.
            routes.push(FleetRoute {
                provider: *provider,
                prompt: String::new(),
            });
        }
        FleetSendPlan {
            routes,
            messages_appended: appended,
        }
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
        return Vec::new();
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

/// Shell line that starts the provider CLI. Prefer bare binary — stuffing the
/// fleet message as argv looked like “tabs full of text” and slowed startup.
pub(crate) fn launch_shell_for_route(provider: AgentProviderId, prompt: &str) -> String {
    let _ = prompt; // reserved for future inject-into-running-session
    provider.launch_command().to_string()
}

#[cfg(test)]
#[path = "agent_fleet_room_tests.rs"]
mod tests;
