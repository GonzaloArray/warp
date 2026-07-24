//! Map external agent brand identities (Claude / Codex / Grok / …) onto Warp's
//! **native** Agent Mode model providers — not CLI binaries.
//!
//! This is the product path for Local Agents as first-class Warp providers:
//! open an Agent Mode conversation with the right LLM, not a terminal running
//! `claude`/`codex`/`grok`.

use crate::ai::llms::{LLMInfo, LLMPreferences, LLMProvider};
use crate::workspace::agent_provider_hub::AgentProviderId;
use warpui::AppContext;

/// Warp LLM provider behind a fleet member brand, when we have a native path.
pub(crate) fn llm_provider_for_agent(id: AgentProviderId) -> Option<LLMProvider> {
    match id {
        AgentProviderId::Claude => Some(LLMProvider::Anthropic),
        AgentProviderId::Codex => Some(LLMProvider::OpenAI),
        AgentProviderId::Grok => Some(LLMProvider::Xai),
        AgentProviderId::Gemini => Some(LLMProvider::Google),
        // Cursor / Copilot / Hermes / etc. stay CLI-only until there is a native
        // model family in Agent Mode for them.
        AgentProviderId::Kimi
        | AgentProviderId::MiniMax
        | AgentProviderId::OpenCode
        | AgentProviderId::Hermes
        | AgentProviderId::Cursor
        | AgentProviderId::Copilot => None,
    }
}

/// Whether this brand can run as a native Warp Agent Mode conversation.
pub(crate) fn is_native_agent_provider(id: AgentProviderId) -> bool {
    llm_provider_for_agent(id).is_some()
}

/// Pick the best available Agent Mode model for a brand.
///
/// Order:
/// 1. Codex special-case: server `preferred_codex_model`
/// 2. First enabled agent-mode choice matching the provider
/// 3. None if the catalog has nothing for that provider
pub(crate) fn preferred_model_for_agent_provider<'a>(
    prefs: &'a LLMPreferences,
    id: AgentProviderId,
    app: &AppContext,
) -> Option<&'a LLMInfo> {
    match id {
        AgentProviderId::Codex => {
            if let Some(info) = prefs.get_preferred_codex_model() {
                return Some(info);
            }
        }
        _ => {}
    }
    let llm_provider = llm_provider_for_agent(id)?;
    prefs
        .get_base_llm_choices_for_agent_mode(app)
        .find(|info| info.provider == llm_provider && info.disable_reason.is_none())
}

/// User-facing short status for the fleet member list.
pub(crate) fn native_ready_label(
    id: AgentProviderId,
    prefs: &LLMPreferences,
    app: &AppContext,
) -> &'static str {
    if !is_native_agent_provider(id) {
        return "solo CLI";
    }
    if preferred_model_for_agent_provider(prefs, id, app).is_some() {
        "nativo"
    } else {
        "sin modelo"
    }
}

#[cfg(test)]
#[path = "native_agent_provider_tests.rs"]
mod tests;
