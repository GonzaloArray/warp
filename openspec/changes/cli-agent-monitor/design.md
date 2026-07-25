## Context

Warp OSS already tracks per-pane CLI agents via `CLIAgent` + `CLIAgentSessionsModel` (command detection, OSC plugins, statuses InProgress/Success/Failed/Blocked). What is missing is a **global ADHD-oriented review memory**: finished sessions stay green until the user successfully focuses the associated pane (or marks reviewed when the pane is gone).

This product is a WarpUI terminal—not Hyprland/QuickShell. Navigation is pane focus; the “bar” is the vertical-tabs header toolbar.

## Goals / Non-Goals

**Goals:**

- Durable monitor states with priority `ERROR → WAITING → COMPLETED_UNSEEN → RUNNING → REVIEWED` (`UNKNOWN` last / low).
- Common adapter for Claude / Codex / Grok (+ future agents).
- Persistence of `COMPLETED_UNSEEN` independent of process lifetime.
- Compact toolbar indicator + single-column panel.
- Focus existing pane; confirm before `REVIEWED`.
- Unit-tested pure domain logic.

**Non-Goals:**

- Chat, prompt injection, remote control, multi-agent orchestration.
- Hardcoding Kitty/Foot/Alacritty or implementing hyprctl.
- Auto-discard finished sessions on terminal/process death.
- Full GUI E2E as sole verification.

## Decisions

### D1 — Domain module separate from UI

**Choice:** `app/src/terminal/cli_agent_monitor/` with pure types: `MonitorState`, `MonitorSession`, `MonitorStore`, `AgentAdapter`, priority sort, review transitions.

**Why:** Unit tests drive shipped functions without GUI/PTY. UI only projects store snapshots.

**Alt:** Only extend `CLIAgentSessionStatus` — rejected: does not encode review memory across restarts.

### D2 — Source of truth for live status vs review

**Choice:** Live agent signals come from `CLIAgentSessionsModel` (and adapters). **Review state** (`COMPLETED_UNSEEN` / `REVIEWED`) lives only in the monitor store. Mapping:

| CLIAgentSessionStatus | MonitorState (if not already REVIEWED) |
|-----------------------|----------------------------------------|
| InProgress | RUNNING |
| Blocked | WAITING |
| Failed | ERROR |
| Success | COMPLETED_UNSEEN (sticky until review) |

**Why:** Avoid fighting ambient task sync; ADHD rule is monitor-owned.

### D3 — Persistence format

**Choice:** JSON file under `warp_core::paths::data_dir()/cli_agent_monitor/sessions.json` (serde). Atomic write (temp + rename). In-memory store for tests with injectable filesystem trait or pure `MonitorStore` struct.

**Why:** Fast to ship and test; Diesel schema migration not required for v1.

### D4 — Session identity

**Choice:** Stable id = plugin `session_id` when present, else `{agent}:{terminal_view_id}:{started_at}` or content hash of (agent, cwd, terminal_view_id, first_seen). Keep `terminal_view_id` / optional `pane_id` for navigation; on pane death keep summary without requiring live EntityId validity.

### D5 — Detection adapters

**Choice:** `trait AgentAdapter { fn match_process(...) -> Option<AgentKind>; fn agent_kind() -> AgentKind; }`. Built-ins: Claude (`claude`), Codex (`codex`), Grok (`grok`). Product path also uses `CLIAgent::detect` + session events. Add `CLIAgent::Grok`.

### D6 — UI surface

**Choice:** New `HeaderToolbarItemKind::CliAgentMonitor` on default right (near notifications). Compact label `N agentes · M para revisar`. Panel: single-column list; colors per state. Click row → focus attempt → review or “Marcar como revisado”.

### D7 — Notifications

**Choice:** On first transition into `COMPLETED_UNSEEN`, send `UserNotification` via existing WarpUI path. Notification dismiss must **not** mark reviewed. Optional `NotificationContext` variant for deep-link later; v1 may only notify.

### D8 — Navigation confirm

**Choice:** `NavigationResult { Focused | MissingTerminal | Failed }` pure function + workspace glue that calls `reveal_and_focus_pane` / `focus_pane_by_id` and returns Focused only if pane still exists and focus API succeeds. Domain `apply_navigation_result` only then → REVIEWED.

## Risks / Trade-offs

- [Double systems with ambient tasks] → Monitor is review-memory only; document boundary.
- [Header toolbar serialization] → Additive enum variant; tests for defaults/all_items.
- [EntityId invalid after restart] → Persist human summary; navigation may fail → explicit mark path.
- [Grok signals weak] → Command-prefix adapter + hooks if needed; still in scope.
- [workspace/view.rs size] → Thin render/action methods; logic in monitor module.

## Migration Plan

- Additive feature; empty store file is valid.
- Rollback: remove toolbar item / module; leave JSON on disk harmless.

## Open Questions

- None blocking v1; optional feature flag if dogfood prefers gated rollout (default on for dogfood via FeatureFlag if team convention requires).
