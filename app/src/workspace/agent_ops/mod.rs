//! Agent Operations domain: multi-dimensional state, ordered events, and alerts.
//!
//! Pure logic for the ops alert center. Warp UI (`vertical_tabs`, projection)
//! consumes derived views; this module does not parse PTY text or invent hierarchy.

mod alerts;
mod events;
mod policy;
mod projection_bridge;
mod runtime;
mod state;
mod store;

#[cfg(test)]
#[path = "alerts_tests.rs"]
mod alerts_tests;
#[cfg(test)]
#[path = "events_tests.rs"]
mod events_tests;
#[cfg(test)]
#[path = "policy_tests.rs"]
mod policy_tests;
#[cfg(test)]
#[path = "projection_bridge_tests.rs"]
mod projection_bridge_tests;
#[cfg(test)]
#[path = "runtime_tests.rs"]
mod runtime_tests;
#[cfg(test)]
#[path = "state_tests.rs"]
mod state_tests;
#[cfg(test)]
#[path = "store_tests.rs"]
mod store_tests;

pub(crate) use alerts::{
    Alert, AlertCategory, AlertLifecycle, AlertSeverity, AlertStore, NavigationTarget,
};
pub(crate) use events::{
    AgentOpsEvent, EventAuthority, EventEnvelope, EventSource, SignalPriority,
};
pub(crate) use policy::{
    EscalationPolicy, SuppressContext, escalate_severity, should_suppress_new, tick_alerts,
};
pub(crate) use projection_bridge::{
    AlertCenterRow, AttentionStripItem, alert_center_rows, build_attention_strip,
    draft_alert_for_execution, format_visible_badge, monitor_node_for_agent_id,
    ops_agent_id_for_node, resolve_visible_status, visible_from_tab_status,
};
pub(crate) use runtime::{
    AgentRuntime, DaytonaSandboxRuntime, LocalPtyRuntime, RuntimeReport, SshRuntime,
};
pub(crate) use state::{
    AttentionState, ExecutionState, RuntimeKind, RuntimeState, VerificationState, VisibleStatus,
    derive_attention_rank, derive_visible_status, map_tab_status_to_execution,
};
pub(crate) use store::{AgentOpsSnapshot, AgentOpsStore};
