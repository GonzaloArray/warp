//! Monitor session states and priority ordering for the CLI agent monitor.

use serde::{Deserialize, Serialize};

/// ADHD-oriented monitor state for a CLI agent session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorState {
    Running,
    Waiting,
    CompletedUnseen,
    Reviewed,
    Error,
    Unknown,
}

impl MonitorState {
    /// Priority for display sort: lower number = higher priority.
    /// ERROR → WAITING → COMPLETED_UNSEEN → RUNNING → REVIEWED → UNKNOWN
    pub fn priority(self) -> u8 {
        match self {
            Self::Error => 0,
            Self::Waiting => 1,
            Self::CompletedUnseen => 2,
            Self::Running => 3,
            Self::Reviewed => 4,
            Self::Unknown => 5,
        }
    }

    pub fn is_pending_review(self) -> bool {
        matches!(self, Self::CompletedUnseen)
    }

    /// Semantic color token name for UI (not a pathfinder color — UI maps tokens).
    pub fn color_token(self) -> &'static str {
        match self {
            Self::CompletedUnseen => "green",
            Self::Waiting => "yellow",
            Self::Error => "red",
            Self::Running => "neutral_animated",
            Self::Reviewed => "gray",
            Self::Unknown => "gray",
        }
    }

    pub fn display_label_es(self) -> &'static str {
        match self {
            Self::Running => "Trabajando",
            Self::Waiting => "Esperando",
            Self::CompletedUnseen => "Terminado — revisar",
            Self::Reviewed => "Revisado",
            Self::Error => "Error",
            Self::Unknown => "Desconocido",
        }
    }
}

/// Sort key: state priority, then older first within same priority.
pub fn compare_sessions_by_priority(
    a_state: MonitorState,
    a_started_ms: u64,
    b_state: MonitorState,
    b_started_ms: u64,
) -> std::cmp::Ordering {
    a_state
        .priority()
        .cmp(&b_state.priority())
        .then_with(|| a_started_ms.cmp(&b_started_ms))
}
