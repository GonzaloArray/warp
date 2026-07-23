use super::*;
use crate::workspace::agent_tabs_projection::AgentTabStatus;

#[test]
fn rejects_uuid_and_json_as_unspeakable() {
    assert!(!is_speakable_body(
        "019f7d73-aaaa-7a02-b496-0476e738eeac"
    ));
    assert!(!is_speakable_body(
        r#"{"type":"function_call","name":"bash"}"#
    ));
    assert!(!is_speakable_title("toolu_01AbCdEfGhIjKlMn"));
    assert!(!is_speakable_body("No se realizaron cambios en archivos."));
}

#[test]
fn accepts_human_task_phrases() {
    assert!(is_speakable_body(
        "Explorar el módulo de webhooks del core"
    ));
    assert!(is_speakable_title("webhooks-core"));
}

#[test]
fn working_with_task_and_web_activity_speaks_progressively() {
    let v = SubagentVoice::compose(&SubagentVoiceInput {
        status: AgentTabStatus::Working,
        display_label: "auth_oauth".into(),
        task_summary: Some("Tarea: auth OAuth flow".into()),
        activity: Some("web".into()),
        ..Default::default()
    });
    assert_eq!(v.title, "auth_oauth");
    assert!(
        v.speaking.contains("Estoy buscando en la web"),
        "got {}",
        v.speaking
    );
    assert!(
        v.speaking.contains("auth OAuth") || v.speaking.contains("para"),
        "got {}",
        v.speaking
    );
    assert!(v.meta.starts_with("trabajando"));
    assert!(v.card_subtitle.contains(&v.speaking));
}

#[test]
fn completed_with_outcome_uses_listo_frame() {
    let v = SubagentVoice::compose(&SubagentVoiceInput {
        status: AgentTabStatus::Completed,
        display_label: "router-tests".into(),
        task_summary: Some("Tarea: tests del router".into()),
        result_summary: Some("Tests del router en verde".into()),
        files_changed_count: 3,
        elapsed_label: Some("2m".into()),
        ..Default::default()
    });
    assert!(v.speaking.starts_with("Listo ·"), "got {}", v.speaking);
    assert!(v.speaking.contains("verde"), "got {}", v.speaking);
    assert!(v.meta.contains("terminado"));
    assert!(v.meta.contains("3 archivo"));
    assert_eq!(
        v.objective.as_deref(),
        Some("tests del router")
    );
}

#[test]
fn blocked_without_task_is_permission_ask() {
    let v = SubagentVoice::compose(&SubagentVoiceInput {
        status: AgentTabStatus::Blocked,
        display_label: "patcher".into(),
        ..Default::default()
    });
    assert_eq!(v.speaking, "Necesito permiso");
    assert!(v.meta.starts_with("bloqueado"));
}

#[test]
fn bare_working_without_content_does_not_invent_facts() {
    let v = SubagentVoice::compose(&SubagentVoiceInput {
        status: AgentTabStatus::Working,
        display_label: "worker".into(),
        activity: Some(r#"{"command":"x"}"#.into()),
        task_summary: Some("ab".into()), // too short
        ..Default::default()
    });
    assert_eq!(v.speaking, "Estoy trabajando");
}

#[test]
fn normalize_activity_maps_known_tools() {
    assert_eq!(
        normalize_activity_verb("shell · cargo test -p app").as_deref(),
        Some("corriendo `cargo test -p app`")
    );
    assert_eq!(
        normalize_activity_verb("Read · leyendo archivo").as_deref(),
        Some("leyendo archivos")
    );
    assert!(normalize_activity_verb("herramienta: bash").is_none());
}

#[test]
fn card_subtitle_skips_redundant_status_echo() {
    let v = SubagentVoice::compose(&SubagentVoiceInput {
        status: AgentTabStatus::Working,
        display_label: "x".into(),
        ..Default::default()
    });
    // No speakable task/activity → speaking is generic; subtitle can be meta-only
    // or meta · speaking; must not double-noise with empty inventiveness.
    assert!(
        v.card_subtitle.contains("trabajando") || v.card_subtitle.contains("Estoy"),
        "got {}",
        v.card_subtitle
    );
}
