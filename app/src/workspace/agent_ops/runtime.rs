//! Common runtime reporting contract for Local / SSH / Daytona.

use serde::{Deserialize, Serialize};

use super::state::{RuntimeKind, RuntimeState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RuntimeConnectivity {
    Connected,
    Degraded,
    Disconnected,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RuntimeHealth {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RuntimeLifecycle {
    Provisioning,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

/// Snapshot reported by any AgentRuntime adapter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RuntimeReport {
    pub runtime_id: String,
    pub kind: RuntimeKind,
    pub connectivity: RuntimeConnectivity,
    pub health: RuntimeHealth,
    pub lifecycle: RuntimeLifecycle,
    pub current_process: Option<String>,
    pub last_heartbeat_ms: Option<u64>,
    pub logs_available: bool,
    pub accepts_input: bool,
    pub can_reconnect: bool,
    pub agent_id: Option<String>,
}

impl RuntimeReport {
    pub(crate) fn to_runtime_state(&self) -> RuntimeState {
        match (self.connectivity, self.lifecycle) {
            (_, RuntimeLifecycle::Provisioning | RuntimeLifecycle::Starting) => {
                RuntimeState::Provisioning
            }
            (RuntimeConnectivity::Connected, RuntimeLifecycle::Running)
                if self.health == RuntimeHealth::Healthy =>
            {
                RuntimeState::Online
            }
            (RuntimeConnectivity::Connected | RuntimeConnectivity::Degraded, _)
                if self.health == RuntimeHealth::Degraded =>
            {
                RuntimeState::Degraded
            }
            (RuntimeConnectivity::Disconnected, RuntimeLifecycle::Running)
            | (_, RuntimeLifecycle::Failed) => RuntimeState::Offline,
            (_, RuntimeLifecycle::Stopping | RuntimeLifecycle::Stopped) => RuntimeState::Stopped,
            (RuntimeConnectivity::Disconnected, _) => RuntimeState::Reconnecting,
            _ => RuntimeState::Offline,
        }
    }
}

/// Common contract — implementors are thin adapters over existing Warp sessions.
pub(crate) trait AgentRuntime {
    fn runtime_id(&self) -> &str;
    fn kind(&self) -> RuntimeKind;
    fn report(&self, now_ms: u64) -> RuntimeReport;
}

/// Local PTY / CLI agent session.
#[derive(Clone, Debug)]
pub(crate) struct LocalPtyRuntime {
    pub runtime_id: String,
    pub agent_id: Option<String>,
    pub process_name: Option<String>,
    pub online: bool,
    pub last_heartbeat_ms: Option<u64>,
}

impl AgentRuntime for LocalPtyRuntime {
    fn runtime_id(&self) -> &str {
        &self.runtime_id
    }
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::Local
    }
    fn report(&self, _now_ms: u64) -> RuntimeReport {
        RuntimeReport {
            runtime_id: self.runtime_id.clone(),
            kind: RuntimeKind::Local,
            connectivity: if self.online {
                RuntimeConnectivity::Connected
            } else {
                RuntimeConnectivity::Disconnected
            },
            health: if self.online {
                RuntimeHealth::Healthy
            } else {
                RuntimeHealth::Unhealthy
            },
            lifecycle: if self.online {
                RuntimeLifecycle::Running
            } else {
                RuntimeLifecycle::Stopped
            },
            current_process: self.process_name.clone(),
            last_heartbeat_ms: self.last_heartbeat_ms,
            logs_available: true,
            accepts_input: self.online,
            can_reconnect: false,
            agent_id: self.agent_id.clone(),
        }
    }
}

/// SSH remote session adapter.
#[derive(Clone, Debug)]
pub(crate) struct SshRuntime {
    pub runtime_id: String,
    pub agent_id: Option<String>,
    pub remote_host: String,
    pub connected: bool,
    pub last_heartbeat_ms: Option<u64>,
}

impl AgentRuntime for SshRuntime {
    fn runtime_id(&self) -> &str {
        &self.runtime_id
    }
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::Ssh
    }
    fn report(&self, _now_ms: u64) -> RuntimeReport {
        RuntimeReport {
            runtime_id: self.runtime_id.clone(),
            kind: RuntimeKind::Ssh,
            connectivity: if self.connected {
                RuntimeConnectivity::Connected
            } else {
                RuntimeConnectivity::Disconnected
            },
            health: if self.connected {
                RuntimeHealth::Healthy
            } else {
                RuntimeHealth::Unhealthy
            },
            lifecycle: if self.connected {
                RuntimeLifecycle::Running
            } else {
                RuntimeLifecycle::Stopped
            },
            current_process: Some(format!("ssh {}", self.remote_host)),
            last_heartbeat_ms: self.last_heartbeat_ms,
            logs_available: self.connected,
            accepts_input: self.connected,
            can_reconnect: true,
            agent_id: self.agent_id.clone(),
        }
    }
}

/// Experimental Daytona sandbox (flag/stub — reporting only).
#[derive(Clone, Debug)]
pub(crate) struct DaytonaSandboxRuntime {
    pub runtime_id: String,
    pub agent_id: Option<String>,
    pub sandbox_id: String,
    pub provisioning: bool,
    pub online: bool,
    pub last_heartbeat_ms: Option<u64>,
}

impl AgentRuntime for DaytonaSandboxRuntime {
    fn runtime_id(&self) -> &str {
        &self.runtime_id
    }
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::Daytona
    }
    fn report(&self, _now_ms: u64) -> RuntimeReport {
        let (connectivity, health, lifecycle) = if self.provisioning {
            (
                RuntimeConnectivity::Unknown,
                RuntimeHealth::Unknown,
                RuntimeLifecycle::Provisioning,
            )
        } else if self.online {
            (
                RuntimeConnectivity::Connected,
                RuntimeHealth::Healthy,
                RuntimeLifecycle::Running,
            )
        } else {
            (
                RuntimeConnectivity::Disconnected,
                RuntimeHealth::Unhealthy,
                RuntimeLifecycle::Stopped,
            )
        };
        RuntimeReport {
            runtime_id: self.runtime_id.clone(),
            kind: RuntimeKind::Daytona,
            connectivity,
            health,
            lifecycle,
            current_process: Some(format!("daytona:{}", self.sandbox_id)),
            last_heartbeat_ms: self.last_heartbeat_ms,
            logs_available: self.online,
            accepts_input: self.online,
            can_reconnect: true,
            agent_id: self.agent_id.clone(),
        }
    }
}
