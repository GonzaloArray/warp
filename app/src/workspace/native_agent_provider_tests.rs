use super::*;
use crate::workspace::agent_provider_hub::AgentProviderId;

#[test]
fn maps_core_brands_to_warp_llm_providers() {
    assert_eq!(
        llm_provider_for_agent(AgentProviderId::Claude),
        Some(LLMProvider::Anthropic)
    );
    assert_eq!(
        llm_provider_for_agent(AgentProviderId::Codex),
        Some(LLMProvider::OpenAI)
    );
    assert_eq!(
        llm_provider_for_agent(AgentProviderId::Grok),
        Some(LLMProvider::Xai)
    );
    assert_eq!(
        llm_provider_for_agent(AgentProviderId::Gemini),
        Some(LLMProvider::Google)
    );
    assert_eq!(llm_provider_for_agent(AgentProviderId::Hermes), None);
    assert_eq!(llm_provider_for_agent(AgentProviderId::Cursor), None);
}

#[test]
fn only_core_brands_are_native() {
    assert!(is_native_agent_provider(AgentProviderId::Claude));
    assert!(is_native_agent_provider(AgentProviderId::Codex));
    assert!(is_native_agent_provider(AgentProviderId::Grok));
    assert!(!is_native_agent_provider(AgentProviderId::Hermes));
}
