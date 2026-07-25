## Why

People running Claude Code, Codex CLI, and Grok CLI inside Warp need external working memory (especially with ADHD): which agents are busy, waiting, failed, or finished but not yet reviewed. Finished work must stay visibly “needs review” across restarts and process death until the user actually opens that terminal—or explicitly marks it reviewed if the terminal is gone.

## What Changes

- Add a **CLI agent monitor** domain: session identity, state machine (`RUNNING`, `WAITING`, `COMPLETED_UNSEEN`, `REVIEWED`, `ERROR`, `UNKNOWN`), priority ordering, and durable persistence of review state.
- Detect Claude Code, Codex CLI, and **Grok CLI** via a common adapter surface; integrate with existing `CLIAgent` / `CLIAgentSessionsModel` (process/command + plugin events), not window-title scraping as primary signal.
- Persist `COMPLETED_UNSEEN` so green/pending survives panel close, app restart, terminal close, and process exit.
- On transition to finished: desktop notification, green state, pending count++, persist.
- Mark `REVIEWED` only after successful navigate-to-associated-pane, or explicit “Marcar como revisado” when the terminal no longer exists.
- Compact header-toolbar indicator: `[icon] N agentes · M para revisar`; click opens a single-column panel with agent, project, state, elapsed, review-pending.
- Navigation focuses existing Warp panes (no duplicate terminals); Hyprland/QuickShell are out of scope for this product surface.

## Capabilities

### New Capabilities

- `cli-agent-monitor`: End-to-end monitor—state machine, detection adapters, process↔pane association, persistence, notifications, compact bar + panel UI, navigate-and-review.

### Modified Capabilities

- (none — OpenSpec main specs empty; this is a new capability)

## Impact

- `app/src/terminal/cli_agent_monitor/` (new domain + panel)
- `app/src/terminal/cli_agent.rs` (Grok + detection)
- `app/src/terminal/cli_agent_sessions/` (status hooks into monitor)
- `app/src/workspace/header_toolbar_item.rs`, toolbar settings, `workspace/view.rs` (chip + panel + focus)
- `app/src/notification.rs` (optional notification context for monitor sessions)
- Unit tests for state machine, persistence, adapters, navigation confirmation
- No multi-agent orchestration, chat, or remote control
