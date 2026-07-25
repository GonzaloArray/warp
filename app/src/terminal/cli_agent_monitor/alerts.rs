//! Alert channels for the CLI agent monitor (desktop + WhatsApp).
//!
//! WhatsApp uses CallMeBot by default (personal bot, free setup):
//! https://www.callmebot.com/blog/free-api-whatsapp-messages/
//!
//! Config file: `~/.warp-oss/cli_agent_monitor/alerts.json`
//! Env overrides: `WARP_WHATSAPP_PHONE`, `WARP_WHATSAPP_API_KEY`, `WARP_WHATSAPP_ENABLED=1`

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How WhatsApp messages are delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WhatsAppProvider {
    /// CallMeBot personal WhatsApp API (phone + apikey query params).
    #[default]
    CallMeBot,
}

/// Durable alert preferences for the monitor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertPreferences {
    /// macOS / system notification center.
    pub desktop_enabled: bool,
    /// Play session-quality sounds (Ping human-gate / Glass completed / Basso error)
    /// plus OS notification sound when enabled.
    pub desktop_sound: bool,
    /// Send a WhatsApp message on COMPLETED_UNSEEN (not on every Waiting human-gate).
    pub whatsapp_enabled: bool,
    /// E.164 without `+`, e.g. `5491112345678`.
    pub whatsapp_phone: String,
    /// CallMeBot API key (or Cloud API token later).
    pub whatsapp_api_key: String,
    pub whatsapp_provider: WhatsAppProvider,
}

impl Default for AlertPreferences {
    fn default() -> Self {
        Self {
            desktop_enabled: true,
            desktop_sound: true,
            whatsapp_enabled: false,
            whatsapp_phone: String::new(),
            whatsapp_api_key: String::new(),
            whatsapp_provider: WhatsAppProvider::CallMeBot,
        }
    }
}

impl AlertPreferences {
    pub fn load_or_default(path: &Path) -> Self {
        if path.exists() {
            Self::load_from_path(path).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn load_from_path(path: &Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        serde_json::from_slice(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    pub fn save_to_path(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    /// Apply env overrides on top of file prefs (env wins when set).
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(v) = std::env::var("WARP_WHATSAPP_ENABLED") {
            self.whatsapp_enabled = matches!(
                v.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            );
        }
        if let Ok(phone) = std::env::var("WARP_WHATSAPP_PHONE") {
            if !phone.trim().is_empty() {
                self.whatsapp_phone = phone.trim().to_owned();
            }
        }
        if let Ok(key) = std::env::var("WARP_WHATSAPP_API_KEY") {
            if !key.trim().is_empty() {
                self.whatsapp_api_key = key.trim().to_owned();
            }
        }
        self
    }

    pub fn whatsapp_configured(&self) -> bool {
        !self.whatsapp_phone.trim().is_empty() && !self.whatsapp_api_key.trim().is_empty()
    }

    pub fn whatsapp_ready(&self) -> bool {
        self.whatsapp_enabled && self.whatsapp_configured()
    }
}

pub fn alerts_prefs_path(data_dir: PathBuf) -> PathBuf {
    data_dir.join("cli_agent_monitor").join("alerts.json")
}

/// Build a short WhatsApp-friendly message.
pub fn format_agent_alert_message(title: &str, body: &str, session_id: &str) -> String {
    let short_id = if session_id.len() > 12 {
        format!("{}…", &session_id[..8])
    } else {
        session_id.to_owned()
    };
    format!(
        "🔔 *Sumanos / Warp*\n{title}\n{body}\n_(sesión {short_id})_\nAbrí Warp para revisar."
    )
}

/// CallMeBot URL (no secrets in logs — only for request building).
pub fn callmebot_url(phone: &str, api_key: &str, text: &str) -> String {
    let phone = phone.trim().trim_start_matches('+');
    format!(
        "https://api.callmebot.com/whatsapp.php?phone={}&text={}&apikey={}",
        urlencoding_minimal(phone),
        urlencoding_minimal(text),
        urlencoding_minimal(api_key.trim())
    )
}

fn urlencoding_minimal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Fire-and-forget WhatsApp send. Returns Err with a safe (non-secret) message.
pub async fn send_whatsapp_callmebot(
    phone: String,
    api_key: String,
    text: String,
) -> Result<(), String> {
    if phone.trim().is_empty() || api_key.trim().is_empty() {
        return Err("WhatsApp phone o API key vacíos".into());
    }
    let url = callmebot_url(&phone, &api_key, &text);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("cliente http: {e}"))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("request WhatsApp: {e}"))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if status.is_success() {
        log::info!("WhatsApp alert sent (CallMeBot status={status})");
        Ok(())
    } else {
        // Truncate body so we don't dump huge HTML.
        let snippet: String = body.chars().take(120).collect();
        Err(format!("CallMeBot HTTP {status}: {snippet}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_message_includes_title() {
        let m = format_agent_alert_message("Codex terminó", "warp — pendiente", "abcdefghijklmnop");
        assert!(m.contains("Codex terminó"));
        assert!(m.contains("warp"));
        assert!(m.contains("abcdefg…") || m.contains("abcdefgh"));
    }

    #[test]
    fn callmebot_url_encodes_spaces() {
        let u = callmebot_url("54911", "key1", "hola mundo");
        assert!(u.contains("phone=54911"));
        assert!(u.contains("apikey=key1"));
        assert!(u.contains("hola%20mundo"));
    }

    #[test]
    fn whatsapp_ready_requires_enable_and_credentials() {
        let mut p = AlertPreferences::default();
        assert!(!p.whatsapp_ready());
        p.whatsapp_enabled = true;
        p.whatsapp_phone = "54911".into();
        p.whatsapp_api_key = "secret".into();
        assert!(p.whatsapp_ready());
    }
}
