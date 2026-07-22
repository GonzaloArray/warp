//! User-created **Agent projects**: accordion roots in the agents rail.
//!
//! `+` → Nuevo agente creates a project with task sections. CLI workers
//! (Claude, Codex, Grok…) hang under tasks — not as top-level rail rows.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use warpui::EntityId;

const SCHEMA_VERSION: u32 = 1;

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A worker CLI attached to a task (pane-scoped while alive).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AgentWorkerRef {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_view_id: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// A task section inside an agent accordion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AgentTask {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub workers: Vec<AgentWorkerRef>,
    #[serde(default)]
    pub status: AgentTaskStatus,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AgentTaskStatus {
    #[default]
    Todo,
    Doing,
    Done,
    Blocked,
}

/// Default team tabs seeded into every new agent accordion.
pub(crate) const DEFAULT_TEAM_TABS: &[(&str, &str)] = &[
    ("claude", "Claude"),
    ("codex", "Codex"),
    ("grok", "Grok"),
];

impl AgentTask {
    pub(crate) fn new(title: impl Into<String>) -> Self {
        Self {
            id: format!("task-{}", now_ms()),
            title: {
                let t = title.into().trim().to_string();
                if t.is_empty() {
                    "Nueva tarea".into()
                } else {
                    t
                }
            },
            workers: Vec::new(),
            status: AgentTaskStatus::Todo,
        }
    }

    /// Task tab pre-filled with a single CLI worker (Claude / Codex / Grok…).
    pub(crate) fn team_tab(provider: &str, label: &str) -> Self {
        let mut task = Self::new(label);
        // Stable id so expands/selection survive reloads within a session.
        task.id = format!("tab-{provider}");
        task.workers.push(AgentWorkerRef {
            provider: provider.to_string(),
            terminal_view_id: None,
            label: Some(label.to_string()),
        });
        task
    }
}

/// User-created agent (accordion root).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AgentProject {
    pub id: String,
    pub display_name: String,
    pub profile_key: String,
    #[serde(default)]
    pub goal: Option<String>,
    /// Task sections under the accordion (primary structure).
    #[serde(default)]
    pub tasks: Vec<AgentTask>,
    /// Legacy flat workers (migrated into first task on load if tasks empty).
    #[serde(default)]
    pub workers: Vec<AgentWorkerRef>,
    pub created_at_ms: u64,
    #[serde(default)]
    pub updated_at_ms: u64,
}

impl AgentProject {
    /// Creates an agent accordion already populated with Claude / Codex / Grok tabs.
    pub(crate) fn new(display_name: impl Into<String>) -> Self {
        let id = format!("ap-{}", now_ms());
        let display_name = {
            let n = display_name.into().trim().to_string();
            if n.is_empty() {
                "Nuevo agente".into()
            } else {
                n
            }
        };
        let now = now_ms();
        let tasks = DEFAULT_TEAM_TABS
            .iter()
            .map(|(provider, label)| AgentTask::team_tab(provider, label))
            .collect();
        Self {
            id: id.clone(),
            display_name,
            profile_key: id,
            goal: None,
            tasks,
            workers: Vec::new(),
            created_at_ms: now,
            updated_at_ms: now,
        }
    }

    pub(crate) fn profile_store_key(&self) -> String {
        format!("project:{}", self.profile_key)
    }

    pub(crate) fn add_task(&mut self, title: impl Into<String>) -> &AgentTask {
        self.tasks.push(AgentTask::new(title));
        self.updated_at_ms = now_ms();
        self.tasks.last().expect("just pushed")
    }

    pub(crate) fn attach_worker_to_task(
        &mut self,
        task_id: &str,
        provider: impl Into<String>,
        terminal_view_id: EntityId,
        label: Option<String>,
    ) -> bool {
        let tid = format!("{terminal_view_id}").parse::<usize>().ok();
        self.push_worker_on_task(
            task_id,
            AgentWorkerRef {
                provider: provider.into(),
                terminal_view_id: tid,
                label,
            },
        )
    }

    /// Slot a worker under a task without a live terminal yet (rail planning).
    pub(crate) fn add_worker_slot(
        &mut self,
        task_id: &str,
        provider: impl Into<String>,
    ) -> bool {
        let provider = {
            let p = provider.into().trim().to_string();
            if p.is_empty() {
                return false;
            }
            p
        };
        let label = Some(provider.clone());
        self.push_worker_on_task(
            task_id,
            AgentWorkerRef {
                provider,
                terminal_view_id: None,
                label,
            },
        )
    }

    fn push_worker_on_task(&mut self, task_id: &str, worker: AgentWorkerRef) -> bool {
        let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) else {
            return false;
        };
        task.workers.push(worker);
        self.updated_at_ms = now_ms();
        true
    }

    /// Normalize legacy `workers` into the first task.
    pub(crate) fn migrate_legacy_workers(&mut self) {
        if self.workers.is_empty() {
            return;
        }
        if self.tasks.is_empty() {
            self.tasks.push(AgentTask::new("Nueva tarea"));
        }
        if let Some(first) = self.tasks.first_mut() {
            first.workers.append(&mut self.workers);
        }
        self.workers.clear();
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct AgentProjectStore {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub projects: BTreeMap<String, AgentProject>,
}

impl AgentProjectStore {
    pub(crate) fn default_path() -> PathBuf {
        warp_core::paths::config_local_dir().join("agent-projects.json")
    }

    pub(crate) fn load_default() -> Self {
        let mut store = Self::load(&Self::default_path());
        for p in store.projects.values_mut() {
            p.migrate_legacy_workers();
            if p.tasks.is_empty() {
                p.tasks.push(AgentTask::new("Nueva tarea"));
            }
        }
        store
    }

    pub(crate) fn load(path: &Path) -> Self {
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    pub(crate) fn save_default(&self) -> std::io::Result<()> {
        self.save(&Self::default_path())
    }

    pub(crate) fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_vec_pretty(self).expect("AgentProjectStore serializable");
        fs::write(path, data)
    }

    pub(crate) fn insert(&mut self, project: AgentProject) {
        self.projects.insert(project.id.clone(), project);
    }

    pub(crate) fn get(&self, id: &str) -> Option<&AgentProject> {
        self.projects.get(id)
    }

    pub(crate) fn get_mut(&mut self, id: &str) -> Option<&mut AgentProject> {
        self.projects.get_mut(id)
    }

    pub(crate) fn list_sorted(&self) -> Vec<&AgentProject> {
        let mut v: Vec<_> = self.projects.values().collect();
        v.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_project_is_accordion_with_team_tabs() {
        let p = AgentProject::new("  Integrar Daytona  ");
        assert_eq!(p.display_name, "Integrar Daytona");
        assert!(p.id.starts_with("ap-"));
        assert_eq!(p.tasks.len(), 3);
        let titles: Vec<_> = p.tasks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["Claude", "Codex", "Grok"]);
        let providers: Vec<_> = p
            .tasks
            .iter()
            .map(|t| t.workers[0].provider.as_str())
            .collect();
        assert_eq!(providers, ["claude", "codex", "grok"]);
    }

    #[test]
    fn add_task_grows_sections() {
        let mut p = AgentProject::new("A");
        p.add_task("Investigar");
        p.add_task("Implementar");
        assert_eq!(p.tasks.len(), 5);
    }

    #[test]
    fn store_roundtrip_keeps_tasks() {
        let mut store = AgentProjectStore::default();
        let mut p = AgentProject::new("QA");
        p.add_task("E2E");
        let id = p.id.clone();
        store.insert(p);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.json");
        store.save(&path).unwrap();
        let loaded = AgentProjectStore::load(&path);
        assert_eq!(loaded.get(&id).unwrap().tasks.len(), 4);
    }
}
