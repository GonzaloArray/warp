//! Escalation, suppression, and expiration policies (pure, clock-injected).

use super::alerts::{Alert, AlertCategory, AlertLifecycle, AlertSeverity, AlertStore};

/// Configurable thresholds (ms). Defaults match product examples.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EscalationPolicy {
    pub waiting_input_medium_ms: u64,
    pub waiting_input_high_ms: u64,
    pub blocked_critical_ms: u64,
    pub reconnecting_high_after_count: u32,
    pub test_failed_high_after_count: u32,
    pub default_expire_ms: u64,
}

impl Default for EscalationPolicy {
    fn default() -> Self {
        Self {
            waiting_input_medium_ms: 30_000,
            waiting_input_high_ms: 300_000,
            blocked_critical_ms: 900_000,
            reconnecting_high_after_count: 3,
            test_failed_high_after_count: 3,
            default_expire_ms: 86_400_000, // 24h
        }
    }
}

/// Context for suppression decisions (injected by callers).
#[derive(Clone, Debug, Default)]
pub(crate) struct SuppressContext {
    /// Agent currently focused in the rail / terminal.
    pub focused_agent_id: Option<String>,
    /// User can already see the prompt (suppress input-required noise).
    pub prompt_visible: bool,
    /// Task was intentionally cancelled.
    pub intentional_cancel: bool,
    /// Runtime stopped as cleanup, not failure.
    pub intentional_runtime_stop: bool,
    /// Keys temporarily silenced.
    pub silenced_keys: Vec<String>,
}

/// Pure escalation: returns new severity if it should increase, else None.
pub(crate) fn escalate_severity(
    alert: &Alert,
    now_ms: u64,
    policy: &EscalationPolicy,
) -> Option<AlertSeverity> {
    if !alert.is_open() {
        return None;
    }
    let age = now_ms.saturating_sub(alert.created_at_ms);
    let target = match alert.category {
        AlertCategory::InputRequired | AlertCategory::ApprovalRequired => {
            if age >= policy.waiting_input_high_ms {
                Some(AlertSeverity::High)
            } else if age >= policy.waiting_input_medium_ms {
                Some(AlertSeverity::Medium)
            } else {
                None
            }
        }
        AlertCategory::AgentBlocked => {
            if age >= policy.blocked_critical_ms {
                Some(AlertSeverity::Critical)
            } else {
                None
            }
        }
        AlertCategory::RuntimeDisconnected | AlertCategory::RuntimeDegraded => {
            if alert.occurrence_count >= policy.reconnecting_high_after_count {
                Some(AlertSeverity::High)
            } else {
                Some(AlertSeverity::Low)
            }
        }
        AlertCategory::TestFailed => {
            if alert.occurrence_count >= policy.test_failed_high_after_count {
                Some(AlertSeverity::High)
            } else {
                Some(AlertSeverity::Medium)
            }
        }
        AlertCategory::IterationLimit | AlertCategory::TimeLimit | AlertCategory::BudgetLimit => {
            Some(AlertSeverity::Critical)
        }
        _ => None,
    }?;
    if target > alert.severity {
        Some(target)
    } else {
        None
    }
}

/// Whether a new alert of this shape should be suppressed (not created).
pub(crate) fn should_suppress_new(
    category: AlertCategory,
    agent_id: Option<&str>,
    dedupe_key: &str,
    store: &AlertStore,
    ctx: &SuppressContext,
) -> bool {
    if ctx.intentional_cancel {
        return true;
    }
    if ctx.intentional_runtime_stop
        && matches!(
            category,
            AlertCategory::RuntimeDisconnected | AlertCategory::HeartbeatLost
        )
    {
        return true;
    }
    if ctx.silenced_keys.iter().any(|k| k == dedupe_key) {
        return true;
    }
    // Equivalent open alert already exists (caller still may upsert for count).
    if store
        .open_alerts()
        .iter()
        .any(|a| a.deduplication_key == dedupe_key)
    {
        // Do not block upsert/merge — only suppress *new toast* channels.
        // For create gate: allow upsert path; suppress only when focused+visible prompt.
    }
    if ctx.prompt_visible
        && agent_id.is_some()
        && ctx.focused_agent_id.as_deref() == agent_id
        && matches!(
            category,
            AlertCategory::InputRequired | AlertCategory::ApprovalRequired
        )
    {
        return true;
    }
    false
}

/// Apply time-based escalation + expiration across open alerts.
pub(crate) fn tick_alerts(
    store: &mut AlertStore,
    now_ms: u64,
    policy: &EscalationPolicy,
) -> (u32 /* escalated */, u32 /* expired */) {
    let mut escalated = 0u32;
    let mut expired = 0u32;
    let ids: Vec<String> = store
        .open_alerts()
        .into_iter()
        .map(|a| a.alert_id.clone())
        .collect();
    for id in ids {
        let Some(alert) = store.get(&id).cloned() else {
            continue;
        };
        if let Some(sev) = escalate_severity(&alert, now_ms, policy) {
            if store.set_severity(&id, sev, now_ms) {
                escalated = escalated.saturating_add(1);
            }
        }
        let age = now_ms.saturating_sub(alert.created_at_ms);
        if age >= policy.default_expire_ms
            && matches!(
                alert.lifecycle,
                AlertLifecycle::Acknowledged | AlertLifecycle::Delivered
            )
        {
            if store.expire(&id, now_ms) {
                expired = expired.saturating_add(1);
            }
        }
    }
    (escalated, expired)
}
