//! Local, non-secret identities used by the agent monitor.

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct AgentProfileStore {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub profiles: BTreeMap<String, AgentProfile>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct AgentProfile {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub agent_key: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub icon: AvatarKind,
    #[serde(default)]
    pub palette: Palette,
}

/// The monitor deliberately accepts only icons compiled into Warp.  Keeping
/// this an enum prevents profile files from becoming a path/URL upload API.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AvatarKind {
    Initial,
    #[default]
    Assistant,
    Code,
    Terminal,
}

impl AvatarKind {
    pub(crate) const ALL: [Self; 4] = [Self::Initial, Self::Assistant, Self::Code, Self::Terminal];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Initial => "Initial",
            Self::Assistant => "Assistant",
            Self::Code => "Code",
            Self::Terminal => "Terminal",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Palette {
    Blue,
    Green,
    Orange,
    Purple,
    Red,
}

impl Default for Palette {
    fn default() -> Self {
        Self::Blue
    }
}

impl Palette {
    pub(crate) const ALL: [Self; 5] = [
        Self::Blue,
        Self::Green,
        Self::Orange,
        Self::Purple,
        Self::Red,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Blue => "Blue",
            Self::Green => "Green",
            Self::Orange => "Orange",
            Self::Purple => "Purple",
            Self::Red => "Red",
        }
    }
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

impl AgentProfile {
    pub(crate) fn new(
        provider: impl Into<String>,
        agent_key: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Option<Self> {
        let provider = provider.into();
        let agent_key = agent_key.into();
        let display_name = display_name.into();
        if valid_key(&provider) && valid_key(&agent_key) {
            Some(Self {
                provider,
                agent_key,
                display_name: sanitize_name(&display_name),
                icon: AvatarKind::default(),
                palette: Palette::default(),
            })
        } else {
            None
        }
    }

    pub(crate) fn key(&self) -> String {
        format!("{}:{}", self.provider, self.agent_key)
    }

    /// Apply user customization while keeping the stable provider/key identity.
    /// Empty names intentionally fall back to the safe default instead of
    /// allowing an unreadable monitor row.
    pub(crate) fn customize(
        &mut self,
        display_name: impl AsRef<str>,
        icon: AvatarKind,
        palette: Palette,
    ) {
        self.display_name = sanitize_name(display_name.as_ref());
        self.icon = icon;
        self.palette = palette;
    }

    fn normalize(mut self) -> Option<Self> {
        if !valid_key(&self.provider) || !valid_key(&self.agent_key) {
            return None;
        }
        self.display_name = sanitize_name(&self.display_name);
        Some(self)
    }
}

impl AgentProfileStore {
    pub(crate) fn upsert(&mut self, profile: AgentProfile) {
        self.profiles.insert(profile.key(), profile);
        self.schema_version = SCHEMA_VERSION;
    }
}

/// Ephemeral state for the monitor's small profile editor.  Keeping this
/// separate from the persisted store makes Cancel a no-op by construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentProfileEditor {
    original: AgentProfile,
    draft: AgentProfile,
}

impl AgentProfileEditor {
    pub(crate) fn begin(profile: &AgentProfile) -> Self {
        Self {
            original: profile.clone(),
            draft: profile.clone(),
        }
    }

    #[allow(dead_code)] // Used when the vertical-tabs profile editor is mounted.
    pub(crate) fn draft(&self) -> &AgentProfile {
        &self.draft
    }

    pub(crate) fn snapshot(&self) -> AgentProfile {
        self.draft.clone()
    }

    pub(crate) fn set_display_name(&mut self, value: impl AsRef<str>) {
        self.draft.display_name = sanitize_name(value.as_ref());
    }

    pub(crate) fn set_icon(&mut self, value: AvatarKind) {
        self.draft.icon = value;
    }

    pub(crate) fn set_palette(&mut self, palette: Palette) {
        self.draft.palette = palette;
    }

    pub(crate) fn cancel(self) -> AgentProfile {
        self.original
    }

    pub(crate) fn save(self, store: &mut AgentProfileStore) {
        store.upsert(self.draft);
    }
}

pub(crate) fn default_profile_path() -> PathBuf {
    warp_core::paths::config_local_dir().join("agent-profiles.json")
}

impl AgentProfileStore {
    pub(crate) fn load_default() -> Self {
        load(&default_profile_path())
    }

    pub(crate) fn save_default(&self) -> std::io::Result<()> {
        save(&default_profile_path(), self)
    }
}

pub(crate) fn load(path: &Path) -> AgentProfileStore {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .filter(|s: &AgentProfileStore| s.schema_version <= SCHEMA_VERSION)
        .map(|mut store: AgentProfileStore| {
            store.profiles = store
                .profiles
                .into_values()
                .filter_map(AgentProfile::normalize)
                .map(|profile| (profile.key(), profile))
                .collect();
            store
        })
        .unwrap_or_default()
}

pub(crate) fn save(path: &Path, store: &AgentProfileStore) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_vec_pretty(store).expect("AgentProfileStore is serializable");
    fs::write(path, data)
}

fn valid_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn sanitize_name(value: &str) -> String {
    let value = value.trim();
    let value = if value.is_empty() { "Agent" } else { value };
    let sanitized: String = value.chars().filter(|c| !c.is_control()).take(64).collect();
    if sanitized.is_empty() {
        "Agent".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_keys_profiles() {
        let profile = AgentProfile::new("oz", "gonzalo", " Gonzalo ").unwrap();
        assert_eq!(profile.key(), "oz:gonzalo");
        assert_eq!(profile.display_name, "Gonzalo");
        assert!(AgentProfile::new("bad key", "id", "x").is_none());
    }

    #[test]
    fn round_trips_restart_and_rejects_future_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent-profiles.json");
        let mut store = AgentProfileStore::default();
        store.upsert(AgentProfile::new("codex", "main", "Codex").unwrap());
        save(&path, &store).unwrap();
        assert_eq!(load(&path), store);
        fs::write(&path, r#"{"schema_version":99,"profiles":{}}"#).unwrap();
        assert_eq!(load(&path), AgentProfileStore::default());
    }

    #[test]
    fn sanitizes_profiles_read_from_disk_and_drops_invalid_identities() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent-profiles.json");
        fs::write(
            &path,
            r#"{
                "schema_version": 1,
                "profiles": {
                    "forged": {
                        "provider": "oz",
                        "agent_key": "main",
                        "display_name": "  Gonzalo\u0000  ",
                        "icon": "code",
                        "palette": "purple"
                    },
                    "bad": {
                        "provider": "bad key",
                        "agent_key": "main",
                        "display_name": "ignored",
                        "icon": "assistant",
                        "palette": "blue"
                    }
                }
            }"#,
        )
        .unwrap();

        let store = load(&path);
        assert_eq!(store.profiles.len(), 1);
        assert_eq!(store.profiles["oz:main"].display_name, "Gonzalo");
    }

    #[test]
    fn customization_is_bounded_and_safe() {
        let mut profile = AgentProfile::new("oz", "main", "Oz").unwrap();
        profile.customize("  Gonzalo  ", AvatarKind::Code, Palette::Purple);
        assert_eq!(profile.display_name, "Gonzalo");
        assert_eq!(profile.icon, AvatarKind::Code);
        assert_eq!(profile.palette, Palette::Purple);

        profile.customize("", AvatarKind::Initial, Palette::Red);
        assert_eq!(profile.display_name, "Agent");
        assert_eq!(profile.icon, AvatarKind::Initial);
    }

    #[test]
    fn avatar_kind_is_a_closed_bundled_allowlist() {
        assert_eq!(
            serde_json::from_str::<AvatarKind>(r#""code""#).unwrap(),
            AvatarKind::Code
        );
        assert!(serde_json::from_str::<AvatarKind>(r#"\"../../secret.png\""#).is_err());
        assert!(serde_json::from_str::<AvatarKind>(r#"\"https://example.com/a.png\""#).is_err());
    }

    #[test]
    fn editor_save_and_cancel_have_expected_semantics() {
        let profile = AgentProfile::new("oz", "main", "Oz").unwrap();
        let mut editor = AgentProfileEditor::begin(&profile);
        editor.set_display_name(" Gonzalo ");
        editor.set_icon(AvatarKind::Code);
        editor.set_palette(Palette::Purple);
        let mut store = AgentProfileStore::default();
        editor.save(&mut store);
        assert_eq!(store.profiles["oz:main"].display_name, "Gonzalo");

        let mut editor = AgentProfileEditor::begin(&profile);
        editor.set_display_name("discarded");
        assert_eq!(editor.cancel().display_name, "Oz");
    }
}
