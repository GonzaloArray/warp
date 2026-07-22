# Spec: Agent Operations Alert Center

**Status:** draft (P0 foundation in progress)  
**Branch:** `feat/agent-task-hierarchy`  
**Change SDD:** `agent-operations-alert-center`  
**Fecha:** 2026-07-21  

## Intent

Convert `vertical_tabs` into the **primary ops navigation** for external (and optional Oz) agents so a human can answer in &lt;5s:

1. Who is working  
2. On which goal/task  
3. Which subagents were delegated  
4. Who is blocked  
5. Who needs human input/approval  
6. What failed  
7. What finished and is still unreviewed  
8. Where each agent runs (local / SSH / Daytona)  
9. Which alerts need immediate attention  

## Principle

> The monitor is not a second surface.  
> **The monitor is navigation.**

Warp observes structured signals, projects hierarchy, and routes to real terminals.  
Warp does **not** invent trees from PTY scrape as authority.

## Continues from

- `docs/specs/external-agent-monitor.md` (CLI roots, Codex/Claude topology, keyboard, providers hub)
- Uncommitted work on `feat/agent-task-hierarchy` (do not re-implement)

## P0 foundation (this iteration)

| Area | Module | Status |
|------|--------|--------|
| Multi-dim state + derived label | `workspace/agent_ops/state.rs` | implemented + unit tests |
| Event envelope + authority | `workspace/agent_ops/events.rs` | implemented + unit tests |
| Alert domain + dedupe + lifecycle | `workspace/agent_ops/alerts.rs` | implemented + unit tests |
| Snapshot store + ordered apply | `workspace/agent_ops/store.rs` | implemented + unit tests |
| Projection bridge (tab status → dims) | `map_tab_status_to_execution` + `projection_bridge` | implemented |
| Rail badges from `derive_visible_status` | `AgentTabNode.ops_*` + `enrich_with_ops` | implemented |
| Attention strip + open alerts in rail | `vertical_tabs` + `build_attention_strip` | implemented |
| CLI session → ops events + alerts | `Workspace::apply_cli_session_to_agent_ops` | implemented (heuristic) |
| Alert JSON in snapshot | `AgentOpsSnapshot.alerts_json` | implemented |
| Alert center full panel (ack UI) | partial (list in strip) | polish later |
| Daytona runtime | — | experimental / later |
| External notification channels | — | out of P0 |

## State model

Four independent dimensions (never collapse into one ambiguous enum):

- **Execution:** provisioning…waiting_input…blocked…completed…failed…  
- **Attention:** none | unseen | seen | action_required  
- **Verification:** not_required | pending | passed | rejected  
- **Runtime:** provisioning | online | degraded | reconnecting | offline | stopped  

Visible badge = pure `derive_visible_status(...)`.

Example: `execution=completed` + `attention=unseen` → **TERMINADO · SIN REVISAR**.

## Signal authority (priority)

1. Native agent event  
2. Orchestrator  
3. Runtime / process  
4. Controlled heuristic  
5. Heartbeat timeout  

Older `sequence` never overwrites newer agent state.  
Weaker authority does not win on equal sequence.

## Alerts

- Severity: info → critical  
- Lifecycle: new → delivered → acknowledged → resolved | dismissed | expired  
- Dedupe key: `project|agent|category|normalized_cause`  
- Storms update `occurrence_count`, do not create N toasts  
- Alerts are not dismissed by showing a toast  

## Hierarchy (target UI)

```
Agent → Goal → Task → Subagent → Execution → Tests/Evidence
```

Collapsed row: name · primary status · time · alert indicator.  
Expand for activity, runtime, progress, children.

## Navigation (already largely from external-agent-monitor)

- ↑/↓ visible rows  
- Space expand  
- Enter activate → real terminal  
- Selection stable under projection refresh (extend with ops events)

## Non-goals (P0)

- Copying Herdr (AGPL)  
- Full Daytona production adapter  
- Slack/Telegram delivery  
- Replacing CLI runtimes  

## Traceability (P0)

| Requirement | Design | Code | Test |
|-------------|--------|------|------|
| Multi-dim status | state model | `agent_ops/state.rs` | `state_tests` |
| Completed unseen | derive_visible | same | `completed_unseen_shows_sin_revisar` |
| Authority / sequence | EventEnvelope | `events.rs` + `store.rs` | `events_tests`, `store_tests` |
| Alert dedupe / storm | AlertStore | `alerts.rs` | `alerts_tests` |
| Lifecycle ack/resolve | AlertStore | `alerts.rs` | `alerts_tests` |
| Persistence snapshot | AgentOpsStore | `store.rs` | `snapshot_roundtrip` |

## Next apply batches

1. Attention strip + per-row badge from `derive_visible_status`  
2. Alert center panel (list open alerts, navigate, ack)  
3. Emit ops events from existing CLI session status transitions  
4. Goal/task nodes when structured topology exposes them  
5. Runtime adapter trait + LocalPty + stub Daytona flag  
