//! Ordered event envelopes with explicit signal authority.

use serde::{Deserialize, Serialize};

/// Who produced the signal (for priority / override rules).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EventSource {
    /// Structured CLI / harness event (Codex JSONL, Claude tool_use, …).
    NativeAgent = 1,
    /// Future orchestrator bus.
    Orchestrator = 2,
    /// Process / SSH / Daytona health.
    Runtime = 3,
    /// Controlled terminal heuristics (never beats native).
    Heuristic = 4,
    /// Timeout from missing heartbeat.
    HeartbeatTimeout = 5,
}

/// Lower number = higher authority (must not be overwritten by weaker/older signals).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) struct EventAuthority(pub u8);

impl EventAuthority {
    pub(crate) fn from_source(source: EventSource) -> Self {
        Self(source as u8)
    }
}

/// Product priority for ranking competing signals of the same type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) struct SignalPriority(pub u8);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub(crate) enum AgentOpsEvent {
    AgentActivityChanged {
        agent_id: String,
        activity: String,
    },
    AgentWaitingInput {
        agent_id: String,
    },
    AgentWaitingApproval {
        agent_id: String,
    },
    AgentBlocked {
        agent_id: String,
        reason: String,
    },
    AgentCompleted {
        agent_id: String,
    },
    AgentFailed {
        agent_id: String,
        reason: String,
    },
    RuntimeDisconnected {
        runtime_id: String,
        agent_id: String,
    },
    RuntimeOnline {
        runtime_id: String,
        agent_id: String,
    },
    GoalProgress {
        agent_id: String,
        goal_id: String,
        /// Human-readable goal title when structured source provides one.
        title: Option<String>,
        completed: u32,
        total: u32,
    },
    AlertCreated {
        alert_id: String,
    },
    AlertAcknowledged {
        alert_id: String,
    },
    AlertResolved {
        alert_id: String,
    },
    AlertDismissed {
        alert_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct EventEnvelope {
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub source: EventSource,
    pub authority: EventAuthority,
    pub confidence: u8,
    pub event: AgentOpsEvent,
}

impl EventEnvelope {
    pub(crate) fn new(
        sequence: u64,
        timestamp_ms: u64,
        source: EventSource,
        event: AgentOpsEvent,
    ) -> Self {
        Self {
            sequence,
            timestamp_ms,
            source,
            authority: EventAuthority::from_source(source),
            confidence: match source {
                EventSource::NativeAgent | EventSource::Orchestrator => 100,
                EventSource::Runtime => 90,
                EventSource::HeartbeatTimeout => 70,
                EventSource::Heuristic => 40,
            },
            event,
        }
    }

    /// Whether `self` should replace prior state established by `prior`.
    pub(crate) fn supersedes(&self, prior: &EventEnvelope) -> bool {
        // Older sequence never overwrites newer state.
        if self.sequence < prior.sequence {
            return false;
        }
        // Equal sequence: stronger authority (lower number) wins.
        if self.sequence == prior.sequence {
            return self.authority < prior.authority;
        }
        // Newer sequence: must have equal or stronger authority.
        // Weaker sources (e.g. heuristic after native) never clobber.
        self.authority.0 <= prior.authority.0
    }
}
