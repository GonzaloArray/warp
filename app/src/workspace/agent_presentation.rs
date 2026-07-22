//! Provider-aware product presentation for external agents.
//!
//! Pure domain: no WarpUI. Encodes what the UI should *know* about each
//! coding agent (Codex / Claude Code / Grok / …) so the rail and session shell
//! stay agentic instead of generic “CLI” chrome.
//!
//! Conceptual inspiration only from agent multiplexors (e.g. Herdr product
//! model). Capabilities are derived from **our** adapters and launch paths.

use crate::workspace::agent_tabs_projection::{AgentTabStatus, ExternalProvider};

/// What the product UI may promise for a given provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AgentUiCapabilities {
    /// Structured parent/child topology (JSONL / tool graph) is trusted.
    pub structured_topology: bool,
    /// In-session deep-dive shell (detail | nav) when children or history exist.
    pub session_shell: bool,
    /// Official continue/resume CLI path exists.
    pub resume_session: bool,
    /// Provider often blocks on permission / human input (rich blocked UX).
    pub permission_prompts: bool,
    /// Subagents/tasks can nest (depth ≥ 2).
    pub nested_hierarchy: bool,
    /// Leaf-only: no children expected; fleet row is enough.
    pub leaf_only: bool,
}

/// Spanish product chrome for one provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AgentUiProfile {
    pub provider: ExternalProvider,
    pub product_name: &'static str,
    pub short_name: &'static str,
    /// Nav section header over the parent card.
    pub section_agent: &'static str,
    /// Title of the parent/root card.
    pub parent_title: &'static str,
    /// Hint when there are no active children.
    pub empty_active_hint: &'static str,
    /// Hint when history is empty.
    pub empty_history_hint: &'static str,
    /// Primary action to return to the parent terminal.
    pub back_to_parent: &'static str,
    /// Word used for children in rollups (“subagents”, “tareas”).
    pub child_noun_singular: &'static str,
    pub child_noun_plural: &'static str,
    /// One-line product truth for empty fleet / tooltips.
    pub capabilities_blurb: &'static str,
    pub capabilities: AgentUiCapabilities,
}

impl AgentUiProfile {
    pub(crate) fn for_provider(provider: ExternalProvider) -> Self {
        match provider {
            ExternalProvider::Codex => Self::codex(),
            ExternalProvider::Claude => Self::claude(),
            ExternalProvider::Grok => Self::grok(),
            ExternalProvider::Gemini => Self::gemini(),
            ExternalProvider::Kimi => Self::kimi(),
            ExternalProvider::MiniMax => Self::minimax(),
            ExternalProvider::Hermes => Self::hermes(),
            ExternalProvider::OpenCode => Self::opencode(),
            ExternalProvider::Cursor => Self::cursor(),
            ExternalProvider::Copilot => Self::copilot(),
            ExternalProvider::Other => Self::other(),
        }
    }

    pub(crate) fn codex() -> Self {
        Self {
            provider: ExternalProvider::Codex,
            product_name: "Codex",
            short_name: "Codex",
            section_agent: "CODEX",
            parent_title: "Sesión principal",
            empty_active_hint: "Cuando Codex lance un subagent, aparece acá con su estado y objetivo.",
            empty_history_hint: "Las ejecuciones de subagents de Codex se archivan acá al terminar.",
            back_to_parent: "← Terminal Codex",
            child_noun_singular: "subagent",
            child_noun_plural: "subagents",
            capabilities_blurb: "Jerarquía real desde rollout JSONL · resume nativo · shell in-session",
            capabilities: AgentUiCapabilities {
                structured_topology: true,
                session_shell: true,
                resume_session: true,
                permission_prompts: true,
                nested_hierarchy: true,
                leaf_only: false,
            },
        }
    }

    pub(crate) fn claude() -> Self {
        Self {
            provider: ExternalProvider::Claude,
            product_name: "Claude Code",
            short_name: "Claude",
            section_agent: "CLAUDE",
            parent_title: "Sesión principal",
            empty_active_hint: "Cuando Claude delegue trabajo (Task / subagents), los ves acá.",
            empty_history_hint: "Tareas y subagents terminados de Claude se archivan acá.",
            back_to_parent: "← Terminal Claude",
            child_noun_singular: "tarea",
            child_noun_plural: "tareas",
            capabilities_blurb: "Topología por tool_use · permisos / preguntas · continue nativo",
            capabilities: AgentUiCapabilities {
                structured_topology: true,
                session_shell: true,
                resume_session: true,
                permission_prompts: true,
                nested_hierarchy: true,
                leaf_only: false,
            },
        }
    }

    pub(crate) fn grok() -> Self {
        Self {
            provider: ExternalProvider::Grok,
            product_name: "Grok",
            short_name: "Grok",
            section_agent: "GROK",
            parent_title: "Sesión Grok",
            empty_active_hint: "Grok corre como hoja en el rail: no hay topología de subagents confiable aún.",
            empty_history_hint: "Sin historial de hijos: el trabajo vive en el terminal Grok.",
            back_to_parent: "← Terminal Grok",
            child_noun_singular: "sesión",
            child_noun_plural: "sesiones",
            capabilities_blurb: "Hoja en el fleet · focus al PTY · sin árbol inventado",
            capabilities: AgentUiCapabilities {
                structured_topology: false,
                session_shell: false,
                resume_session: false,
                permission_prompts: false,
                nested_hierarchy: false,
                leaf_only: true,
            },
        }
    }

    pub(crate) fn gemini() -> Self {
        Self {
            provider: ExternalProvider::Gemini,
            product_name: "Gemini",
            short_name: "Gemini",
            section_agent: "GEMINI",
            parent_title: "Sesión principal",
            empty_active_hint: "Gemini aparece como sesión en el rail. Sin subagents estructurados todavía.",
            empty_history_hint: "Sin historial de hijos estructurados para Gemini.",
            back_to_parent: "← Terminal Gemini",
            child_noun_singular: "sesión",
            child_noun_plural: "sesiones",
            capabilities_blurb: "Sesión en el fleet · resume latest · hoja sin topología",
            capabilities: AgentUiCapabilities {
                structured_topology: false,
                session_shell: false,
                resume_session: true,
                permission_prompts: false,
                nested_hierarchy: false,
                leaf_only: true,
            },
        }
    }

    pub(crate) fn kimi() -> Self {
        leaf_like(
            ExternalProvider::Kimi,
            "Kimi",
            "KIMI",
            "Sesión Kimi en el rail · sin subagents estructurados",
        )
    }

    pub(crate) fn minimax() -> Self {
        leaf_like(
            ExternalProvider::MiniMax,
            "MiniMax",
            "MINIMAX",
            "Sesión MiniMax en el rail · sin subagents estructurados",
        )
    }

    pub(crate) fn hermes() -> Self {
        leaf_like(
            ExternalProvider::Hermes,
            "Hermes",
            "HERMES",
            "Sesión Hermes en el rail · sin subagents estructurados",
        )
    }

    pub(crate) fn opencode() -> Self {
        Self {
            provider: ExternalProvider::OpenCode,
            product_name: "OpenCode",
            short_name: "OpenCode",
            section_agent: "OPENCODE",
            parent_title: "Sesión principal",
            empty_active_hint: "OpenCode en el rail. Topología de hijos aún no confiable.",
            empty_history_hint: "Sin historial de subagents para OpenCode.",
            back_to_parent: "← Terminal OpenCode",
            child_noun_singular: "sesión",
            child_noun_plural: "sesiones",
            capabilities_blurb: "Sesión en el fleet · launch nativo",
            capabilities: AgentUiCapabilities {
                structured_topology: false,
                session_shell: false,
                resume_session: true,
                permission_prompts: false,
                nested_hierarchy: false,
                leaf_only: true,
            },
        }
    }

    pub(crate) fn cursor() -> Self {
        leaf_like(
            ExternalProvider::Cursor,
            "Cursor",
            "CURSOR",
            "Cursor Agent CLI · hoja en el fleet",
        )
    }

    pub(crate) fn copilot() -> Self {
        leaf_like(
            ExternalProvider::Copilot,
            "Copilot",
            "COPILOT",
            "GitHub Copilot CLI · hoja en el fleet",
        )
    }

    pub(crate) fn other() -> Self {
        Self {
            provider: ExternalProvider::Other,
            product_name: "Agent CLI",
            short_name: "CLI",
            section_agent: "AGENTE",
            parent_title: "Sesión principal",
            empty_active_hint: "Cuando este agente exponga hijos estructurados, aparecen acá.",
            empty_history_hint: "Sin historial de hijos todavía.",
            back_to_parent: "← Terminal del agente",
            child_noun_singular: "hijo",
            child_noun_plural: "hijos",
            capabilities_blurb: "Detectado como CLI · sin topología garantizada",
            capabilities: AgentUiCapabilities {
                structured_topology: false,
                session_shell: false,
                resume_session: false,
                permission_prompts: false,
                nested_hierarchy: false,
                leaf_only: true,
            },
        }
    }

    /// Parent card subtitle: `Codex · 2 subagents` / idle line.
    pub(crate) fn parent_subtitle(&self, active_children: usize) -> String {
        if active_children == 0 {
            format!("{} · sin {} en curso", self.short_name, self.child_noun_plural)
        } else if active_children == 1 {
            format!("{} · 1 {}", self.short_name, self.child_noun_singular)
        } else {
            format!(
                "{} · {active_children} {}",
                self.short_name, self.child_noun_plural
            )
        }
    }

    pub(crate) fn section_active_label(&self, count: usize) -> String {
        format!("ACTIVOS · {count}")
    }

    pub(crate) fn section_history_label(&self, count: usize) -> String {
        format!("HISTORIAL · {count}")
    }

    /// Unified product status chip (rail + shell). Short, glanceable Spanish.
    pub(crate) fn status_chip(status: AgentTabStatus, completion_flash: bool) -> &'static str {
        if completion_flash {
            return "terminado";
        }
        match status {
            AgentTabStatus::Working => "trabajando",
            AgentTabStatus::Waiting => "esperando",
            AgentTabStatus::Blocked => "bloqueado",
            AgentTabStatus::Failed => "error",
            AgentTabStatus::Completed => "terminado",
            AgentTabStatus::Unavailable => "iniciando",
        }
    }

    /// Urgency for rollup: higher = more attention. Waiting < Blocked.
    pub(crate) fn status_urgency(status: AgentTabStatus) -> u8 {
        match status {
            AgentTabStatus::Failed => 100,
            AgentTabStatus::Blocked => 90,
            AgentTabStatus::Waiting => 70,
            AgentTabStatus::Working => 50,
            AgentTabStatus::Unavailable => 40,
            AgentTabStatus::Completed => 10,
        }
    }

    /// Worst (highest urgency) status among a set — parent rollup helper.
    pub(crate) fn rollup_status(statuses: impl IntoIterator<Item = AgentTabStatus>) -> Option<AgentTabStatus> {
        statuses
            .into_iter()
            .max_by_key(|s| Self::status_urgency(*s))
    }
}

fn leaf_like(
    provider: ExternalProvider,
    name: &'static str,
    section: &'static str,
    blurb: &'static str,
) -> AgentUiProfile {
    AgentUiProfile {
        provider,
        product_name: name,
        short_name: name,
        section_agent: section,
        parent_title: "Sesión principal",
        empty_active_hint: "Este agente es una hoja en el rail: el trabajo vive en su terminal.",
        empty_history_hint: "Sin historial de subagents para este proveedor.",
        back_to_parent: "← Terminal del agente",
        child_noun_singular: "sesión",
        child_noun_plural: "sesiones",
        capabilities_blurb: blurb,
        capabilities: AgentUiCapabilities {
            structured_topology: false,
            session_shell: false,
            resume_session: false,
            permission_prompts: false,
            nested_hierarchy: false,
            leaf_only: true,
        },
    }
}

/// Product opportunities derived from real capabilities (planning / UI empty states).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentUiOpportunity {
    pub provider: ExternalProvider,
    pub title: &'static str,
    pub detail: &'static str,
    pub phase: OpportunityPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpportunityPhase {
    P0,
    P1,
    P2,
}

/// Static opportunity backlog for agentic UI (Codex / Claude / Grok focus).
pub(crate) fn agentic_ui_opportunities() -> &'static [AgentUiOpportunity] {
    &[
        AgentUiOpportunity {
            provider: ExternalProvider::Codex,
            title: "Shell in-session con marca Codex",
            detail: "Nav + detail + history con copy y jerarquía de subagents reales",
            phase: OpportunityPhase::P0,
        },
        AgentUiOpportunity {
            provider: ExternalProvider::Claude,
            title: "Shell in-session con marca Claude",
            detail: "Misma shell, topología Task/tool_use, permisos como bloqueado",
            phase: OpportunityPhase::P0,
        },
        AgentUiOpportunity {
            provider: ExternalProvider::Grok,
            title: "Fleet leaf honesto",
            detail: "Sin árbol inventado; estado de proceso + focus al PTY",
            phase: OpportunityPhase::P0,
        },
        AgentUiOpportunity {
            provider: ExternalProvider::Codex,
            title: "Screen evidence blocked estricto",
            detail: "OSC title + bottom-buffer propios (no copiar Herdr) para Action Required",
            phase: OpportunityPhase::P1,
        },
        AgentUiOpportunity {
            provider: ExternalProvider::Claude,
            title: "Rich permission → ops attention",
            detail: "PermissionRequest/QuestionAsked ya en CLI events → strip atención",
            phase: OpportunityPhase::P1,
        },
        AgentUiOpportunity {
            provider: ExternalProvider::Grok,
            title: "Mejorar detección de estado",
            detail: "Si Grok expone OSC/hooks, subir authority sin inventar children",
            phase: OpportunityPhase::P1,
        },
        AgentUiOpportunity {
            provider: ExternalProvider::Codex,
            title: "Re-run similar desde historial",
            detail: "Copiar objetivo y re-lanzar subtarea en el padre",
            phase: OpportunityPhase::P2,
        },
    ]
}

#[cfg(test)]
#[path = "agent_presentation_tests.rs"]
mod tests;
