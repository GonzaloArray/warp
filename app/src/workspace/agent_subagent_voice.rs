//! Subagent “voice” presentation — agent-speaking lines with quality gates.
//!
//! Topology stays in `agent_tabs_projection`. This module only turns trusted
//! structured fields into copy that reads like a coding agent reporting to its
//! lead (orchestrator UI), never a raw JSONL dump.

use crate::workspace::agent_tabs_projection::AgentTabStatus;

/// Inputs the voice layer is allowed to consider. All optional text must already
/// come from structured adapters (not terminal scrape).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SubagentVoiceInput {
    pub status: AgentTabStatus,
    pub display_label: String,
    pub task_summary: Option<String>,
    pub activity: Option<String>,
    pub result_summary: Option<String>,
    pub work_summary: Option<String>,
    pub elapsed_label: Option<String>,
    pub relative_event: Option<String>,
    pub files_changed_count: usize,
    pub completion_flash: bool,
}

impl Default for SubagentVoiceInput {
    fn default() -> Self {
        Self {
            status: AgentTabStatus::Working,
            display_label: String::new(),
            task_summary: None,
            activity: None,
            result_summary: None,
            work_summary: None,
            elapsed_label: None,
            relative_event: None,
            files_changed_count: 0,
            completion_flash: false,
        }
    }
}

/// Glanceable + detail copy for one subagent row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SubagentVoice {
    pub title: String,
    /// One sentence, agent voice.
    pub speaking: String,
    /// Facts only: status · time · files.
    pub meta: String,
    /// Compact card/rail subtitle.
    pub card_subtitle: String,
    pub now_line: Option<String>,
    pub objective: Option<String>,
    pub outcome: Option<String>,
}

impl SubagentVoice {
    pub(crate) fn compose(input: &SubagentVoiceInput) -> Self {
        let title = sanitize_title(&input.display_label).unwrap_or_else(|| "Subagent".into());

        let objective = input
            .task_summary
            .as_deref()
            .and_then(clean_objective)
            .filter(|s| is_speakable(s, SpeakKind::Body));

        let outcome = input
            .result_summary
            .as_deref()
            .or(input.work_summary.as_deref())
            .and_then(clean_outcome)
            .filter(|s| is_speakable(s, SpeakKind::Body));

        let activity_verb = input
            .activity
            .as_deref()
            .and_then(normalize_activity_verb);

        let speaking = compose_speaking(
            effective_status(input),
            objective.as_deref(),
            activity_verb.as_deref(),
            outcome.as_deref(),
        );

        let meta = compose_meta(input);
        let card_subtitle = compose_card_subtitle(&meta, &speaking);
        let now_line = match effective_status(input) {
            AgentTabStatus::Completed | AgentTabStatus::Failed => outcome
                .clone()
                .or_else(|| Some(speaking.clone())),
            _ => Some(speaking.clone()),
        };

        Self {
            title,
            speaking,
            meta,
            card_subtitle,
            now_line,
            objective,
            outcome,
        }
    }
}

#[derive(Clone, Copy)]
enum SpeakKind {
    Title,
    Body,
}

fn effective_status(input: &SubagentVoiceInput) -> AgentTabStatus {
    if input.completion_flash {
        AgentTabStatus::Completed
    } else {
        input.status
    }
}

fn compose_speaking(
    status: AgentTabStatus,
    objective: Option<&str>,
    activity_verb: Option<&str>,
    outcome: Option<&str>,
) -> String {
    match status {
        AgentTabStatus::Completed => {
            if let Some(o) = outcome {
                return truncate_chars(&format!("Listo · {o}"), 160);
            }
            if let Some(obj) = objective {
                return truncate_chars(&format!("Listo · {obj}"), 160);
            }
            "Listo".into()
        }
        AgentTabStatus::Failed => {
            if let Some(o) = outcome {
                return truncate_chars(&format!("Fallé · {o}"), 160);
            }
            if let Some(obj) = objective {
                return truncate_chars(&format!("Fallé en {obj}"), 160);
            }
            "Fallé".into()
        }
        AgentTabStatus::Blocked => {
            if let Some(obj) = objective {
                return truncate_chars(&format!("Necesito permiso · {obj}"), 160);
            }
            if let Some(v) = activity_verb {
                return truncate_chars(&format!("Necesito permiso · {v}"), 160);
            }
            "Necesito permiso".into()
        }
        AgentTabStatus::Waiting => {
            if let Some(obj) = objective {
                return truncate_chars(&format!("Espero tu input · {obj}"), 160);
            }
            "Espero tu input".into()
        }
        AgentTabStatus::Unavailable => {
            if let Some(obj) = objective {
                return truncate_chars(&format!("Arrancando · {obj}"), 160);
            }
            "Arrancando".into()
        }
        AgentTabStatus::Working => compose_working(objective, activity_verb),
    }
}

fn compose_working(objective: Option<&str>, activity_verb: Option<&str>) -> String {
    match (activity_verb, objective) {
        (Some(v), Some(obj)) if !verb_covers_objective(v, obj) => {
            truncate_chars(&format!("Estoy {v} para {obj}"), 160)
        }
        (Some(v), _) => truncate_chars(&format!("Estoy {v}"), 160),
        (None, Some(obj)) => truncate_chars(&format!("Estoy trabajando en {obj}"), 160),
        (None, None) => "Estoy trabajando".into(),
    }
}

fn verb_covers_objective(verb: &str, objective: &str) -> bool {
    let v = verb.to_ascii_lowercase();
    let o = objective.to_ascii_lowercase();
    o.contains(&v) || v.contains(&o)
}

fn compose_meta(input: &SubagentVoiceInput) -> String {
    let status = crate::workspace::agent_presentation::AgentUiProfile::status_chip(
        effective_status(input),
        input.completion_flash,
    );
    let mut parts = vec![status.to_string()];
    if let Some(elapsed) = input
        .elapsed_label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        parts.push(elapsed.to_string());
    } else if let Some(rel) = input
        .relative_event
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && is_meta_time(s))
    {
        parts.push(rel.to_string());
    }
    if input.files_changed_count > 0 {
        parts.push(format!(
            "{} archivo{}",
            input.files_changed_count,
            if input.files_changed_count == 1 {
                ""
            } else {
                "s"
            }
        ));
    }
    parts.join(" · ")
}

fn is_meta_time(s: &str) -> bool {
    s.starts_with("hace ") || s == "recién" || s.chars().all(|c| c.is_ascii_digit() || c == 's' || c == 'm' || c == 'h' || c == ' ')
}

fn compose_card_subtitle(meta: &str, speaking: &str) -> String {
    let meta = meta.trim();
    let speaking = speaking.trim();
    if meta.is_empty() {
        return speaking.to_string();
    }
    // Avoid "trabajando · Estoy trabajando"
    if speaking_echoes_status_only(speaking, meta) {
        return meta.to_string();
    }
    format!("{meta} · {speaking}")
}

fn speaking_echoes_status_only(speaking: &str, meta: &str) -> bool {
    let generic = matches!(
        speaking,
        "Estoy trabajando"
            | "Listo"
            | "Fallé"
            | "Necesito permiso"
            | "Espero tu input"
            | "Arrancando"
    );
    if !generic {
        return false;
    }
    // Meta is "status · …" — first chip is enough signal when speaking has no facts.
    meta.split(" · ").next().is_some_and(|chip| {
        matches!(
            chip,
            "trabajando"
                | "terminado"
                | "error"
                | "bloqueado"
                | "esperando"
                | "iniciando"
        )
    })
}

/// Public gate used by tests and optional callers.
pub(crate) fn is_speakable(raw: &str, kind: SpeakKind) -> bool {
    let s = raw.trim();
    let (min, max) = match kind {
        // Short path leaves ("a", "db") are valid titles; body lines need real prose.
        SpeakKind::Title => (1, 48),
        SpeakKind::Body => (8, 160),
    };
    let len = s.chars().count();
    if len < min || len > max {
        return false;
    }
    if is_junk_phrase(s) {
        return false;
    }
    if looks_like_id(s) {
        return false;
    }
    if looks_like_json(s) {
        return false;
    }
    if mostly_non_text(s) {
        return false;
    }
    true
}

// Re-export SpeakKind for tests via is_speakable body/title helpers.
pub(crate) fn is_speakable_body(raw: &str) -> bool {
    is_speakable(raw, SpeakKind::Body)
}

pub(crate) fn is_speakable_title(raw: &str) -> bool {
    is_speakable(raw, SpeakKind::Title)
}

fn is_junk_phrase(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    const JUNK: &[&str] = &[
        "no se realizaron cambios",
        "no se realizaron",
        "system prompt",
        "agents.md",
        "message type:",
        "payload:",
        "null",
        "undefined",
        "herramienta:",
        "function_call",
        "tool_use",
        "event_msg",
        "sub_agent_activity",
    ];
    JUNK.iter().any(|j| lower.contains(j))
}

fn looks_like_id(s: &str) -> bool {
    let t = s.trim();
    // UUID
    if t.len() >= 32
        && t.chars()
            .filter(|c| c.is_ascii_hexdigit() || *c == '-')
            .count()
            == t.len()
    {
        return true;
    }
    if t.starts_with("toolu_") || t.starts_with("agent-") && t.len() > 20 {
        return true;
    }
    // pure hex thread-ish
    if t.len() >= 16 && t.chars().all(|c| c.is_ascii_hexdigit() || c == '-' || c == '_') {
        return true;
    }
    false
}

fn looks_like_json(s: &str) -> bool {
    let t = s.trim();
    if t.starts_with('{') || t.starts_with('[') {
        return true;
    }
    let colon_quotes = t.matches("\":").count() + t.matches("\": ").count();
    colon_quotes >= 2
}

fn mostly_non_text(s: &str) -> bool {
    let total = s.chars().count().max(1);
    let textish = s
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || "áéíóúñüÁÉÍÓÚÑÜ.,;:·/()-_".contains(*c))
        .count();
    (textish * 100 / total) < 55
}

fn sanitize_title(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if looks_like_id(s) {
        return None;
    }
    if looks_like_json(s) {
        return None;
    }
    let leaf = s
        .rsplit('/')
        .find(|p| !p.is_empty())
        .unwrap_or(s)
        .trim();
    if leaf.is_empty() || looks_like_id(leaf) {
        return None;
    }
    let out = truncate_chars(leaf, 48);
    if !out.is_empty() && !looks_like_id(&out) && !looks_like_json(&out) {
        Some(out)
    } else {
        None
    }
}

fn clean_objective(raw: &str) -> Option<String> {
    let mut s = raw.trim().to_string();
    for prefix in ["Tarea:", "Tarea：", "Task:", "Objetivo:", "Payload:"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim().to_string();
        }
    }
    // Drop "Message Type" style first lines
    if s.to_ascii_lowercase().starts_with("message type") {
        return None;
    }
    let s = truncate_chars(s.trim(), 140);
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn clean_outcome(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() || is_junk_phrase(s) {
        return None;
    }
    Some(truncate_chars(s, 140))
}

/// Map raw activity scrap → verb phrase without "Estoy".
pub(crate) fn normalize_activity_verb(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() || looks_like_json(s) || looks_like_id(s) {
        return None;
    }
    let lower = s.to_ascii_lowercase();

    if lower == "web"
        || lower.contains("web search")
        || lower.contains("web__run")
        || lower.contains("buscando en la web")
        || lower.contains("web_search")
    {
        return Some("buscando en la web".into());
    }
    if lower.contains("read_file")
        || lower.contains("leyendo archivo")
        || lower.contains(" · cat ")
        || lower.starts_with("read ")
    {
        return Some("leyendo archivos".into());
    }
    if lower.contains("apply_patch")
        || lower.contains("modificando")
        || lower.contains("write_file")
        || lower.contains("editando")
    {
        return Some("editando el repo".into());
    }
    if lower.contains("grep")
        || lower.contains("buscando en el código")
        || lower.contains("search_code")
        || lower.contains(" · rg")
    {
        return Some("buscando en el código".into());
    }
    if lower.starts_with("shell")
        || lower.contains("cargo ")
        || lower.contains("npm ")
        || lower.contains("pytest")
        || lower.contains("corriendo")
    {
        // Keep a short command hint when present after "shell · "
        if let Some(cmd) = s.split("·").nth(1).map(str::trim).filter(|c| c.len() >= 3 && c.len() <= 48)
        {
            if !looks_like_json(cmd) {
                return Some(format!("corriendo `{cmd}`"));
            }
        }
        return Some("corriendo comandos".into());
    }
    if lower.starts_with("herramienta:") || lower.starts_with("tool:") {
        return None;
    }

    // Already human-ish short phrase
    let cleaned = s
        .trim_start_matches("Ahora · ")
        .trim_start_matches("ahora · ")
        .trim();
    // Strip leading tool name "foo · bar" → prefer bar if speakable
    let candidate = if let Some((name, rest)) = cleaned.split_once(" · ") {
        if name.len() <= 24 && !rest.is_empty() {
            rest.trim()
        } else {
            cleaned
        }
    } else {
        cleaned
    };

    let candidate = candidate.trim();
    if candidate.chars().count() < 4 || is_junk_phrase(candidate) || looks_like_json(candidate) {
        return None;
    }
    // De-noise: if it's already progressive Spanish without "Estoy"
    let verb = if candidate.starts_with("Estoy ") {
        candidate.trim_start_matches("Estoy ").to_string()
    } else {
        candidate.to_string()
    };
    Some(truncate_chars(&verb, 80))
}

fn truncate_chars(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
#[path = "agent_subagent_voice_tests.rs"]
mod tests;
