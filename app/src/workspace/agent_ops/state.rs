//! Multi-dimensional agent state. Visible labels are derived, never stored as truth.

use serde::{Deserialize, Serialize};

use crate::workspace::agent_tabs_projection::AgentTabStatus;

/// Where the agent process/session runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RuntimeKind {
    #[default]
    Local,
    Ssh,
    Daytona,
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ExecutionState {
    Provisioning,
    Starting,
    Working,
    Researching,
    Testing,
    Verifying,
    Retrying,
    WaitingInput,
    WaitingApproval,
    Blocked,
    Completed,
    Failed,
    Cancelled,
    #[default]
    Disconnected,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AttentionState {
    #[default]
    None,
    Unseen,
    Seen,
    ActionRequired,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VerificationState {
    #[default]
    NotRequired,
    Pending,
    Passed,
    Rejected,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RuntimeState {
    Provisioning,
    Online,
    Degraded,
    Reconnecting,
    #[default]
    Offline,
    Stopped,
}

/// Compact UI label derived from dimensions (pure, unit-tested).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VisibleStatus {
    /// Primary badge, e.g. `TRABAJANDO`, `BLOQUEADO`.
    pub primary: &'static str,
    /// Optional secondary badge, e.g. `SIN REVISAR`.
    pub secondary: Option<&'static str>,
    /// Whether the row needs attention strip / alert badge.
    pub needs_attention: bool,
}

/// Map existing monitor status into the execution dimension (best-effort bridge).
pub(crate) fn map_tab_status_to_execution(status: AgentTabStatus) -> ExecutionState {
    match status {
        AgentTabStatus::Working => ExecutionState::Working,
        AgentTabStatus::Waiting => ExecutionState::WaitingInput,
        AgentTabStatus::Blocked => ExecutionState::Blocked,
        AgentTabStatus::Completed => ExecutionState::Completed,
        AgentTabStatus::Failed => ExecutionState::Failed,
        AgentTabStatus::Unavailable => ExecutionState::Disconnected,
    }
}

/// Pure derivation of the human-readable status shown in the rail.
pub(crate) fn derive_visible_status(
    execution: ExecutionState,
    attention: AttentionState,
    verification: VerificationState,
    runtime: RuntimeState,
) -> VisibleStatus {
    // Runtime outages dominate when the process cannot be reached.
    if matches!(
        runtime,
        RuntimeState::Offline | RuntimeState::Reconnecting | RuntimeState::Provisioning
    ) && !matches!(
        execution,
        ExecutionState::Completed | ExecutionState::Failed | ExecutionState::Cancelled
    ) {
        let primary = match runtime {
            RuntimeState::Provisioning => "PROVISIONANDO",
            RuntimeState::Reconnecting => "RECONECTANDO",
            RuntimeState::Offline => "DESCONECTADO",
            _ => "DESCONECTADO",
        };
        return VisibleStatus {
            primary,
            secondary: None,
            needs_attention: !matches!(runtime, RuntimeState::Provisioning),
        };
    }

    let primary = match execution {
        ExecutionState::Provisioning => "PROVISIONANDO",
        ExecutionState::Starting => "INICIANDO",
        ExecutionState::Working => "TRABAJANDO",
        ExecutionState::Researching => "INVESTIGANDO",
        ExecutionState::Testing => "TESTEANDO",
        ExecutionState::Verifying => "VERIFICANDO",
        ExecutionState::Retrying => "REINTENTANDO",
        ExecutionState::WaitingInput => "ESPERA INPUT",
        ExecutionState::WaitingApproval => "ESPERA APROBACIÓN",
        ExecutionState::Blocked => "BLOQUEADO",
        ExecutionState::Completed => "TERMINADO",
        ExecutionState::Failed => "FALLÓ",
        ExecutionState::Cancelled => "CANCELADO",
        ExecutionState::Disconnected => "DESCONECTADO",
    };

    let secondary = match (execution, attention, verification) {
        (ExecutionState::Completed, AttentionState::Unseen, VerificationState::Passed)
        | (ExecutionState::Completed, AttentionState::Unseen, VerificationState::NotRequired) => {
            Some("SIN REVISAR")
        }
        (_, _, VerificationState::Rejected) => Some("VERIF. RECHAZADA"),
        (ExecutionState::Completed, _, VerificationState::Pending) => Some("VERIF. PENDIENTE"),
        (ExecutionState::WaitingApproval, _, _) => Some("ACCIÓN REQUERIDA"),
        (ExecutionState::WaitingInput, _, _) => Some("INPUT"),
        (ExecutionState::Blocked, _, _) => Some("BLOQUEO"),
        _ if attention == AttentionState::ActionRequired => Some("ATENCIÓN"),
        _ => None,
    };

    let needs_attention = matches!(
        execution,
        ExecutionState::WaitingInput
            | ExecutionState::WaitingApproval
            | ExecutionState::Blocked
            | ExecutionState::Failed
    ) || attention == AttentionState::ActionRequired
        || attention == AttentionState::Unseen
            && matches!(execution, ExecutionState::Completed)
        || verification == VerificationState::Rejected
        || matches!(runtime, RuntimeState::Offline | RuntimeState::Degraded);

    VisibleStatus {
        primary,
        secondary,
        needs_attention,
    }
}

/// Lower rank = higher visual priority in the attention strip (stable order).
pub(crate) fn derive_attention_rank(
    execution: ExecutionState,
    attention: AttentionState,
    verification: VerificationState,
    runtime: RuntimeState,
) -> u8 {
    if matches!(
        execution,
        ExecutionState::WaitingApproval
    ) || attention == AttentionState::ActionRequired
        && matches!(execution, ExecutionState::WaitingApproval)
    {
        return 1;
    }
    if matches!(execution, ExecutionState::WaitingInput) {
        return 2;
    }
    if matches!(execution, ExecutionState::Blocked) {
        return 3;
    }
    if matches!(execution, ExecutionState::Failed) {
        return 4;
    }
    if matches!(runtime, RuntimeState::Offline) {
        return 5;
    }
    if verification == VerificationState::Rejected {
        return 6;
    }
    if matches!(execution, ExecutionState::Verifying | ExecutionState::Testing) {
        return 7;
    }
    if matches!(
        execution,
        ExecutionState::Working | ExecutionState::Researching | ExecutionState::Retrying
    ) {
        return 8;
    }
    if matches!(execution, ExecutionState::Completed)
        && attention == AttentionState::Unseen
    {
        return 9;
    }
    10
}
