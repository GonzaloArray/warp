## 1. Domain store

- [x] 1.1 Add `notified_waiting` on `MonitorSession` (serde default false)
- [x] 1.2 Queue human-gate `NotificationRequest` on first → `Waiting`
- [x] 1.3 Keep completed notification; both can fire across lifecycle

## 2. Sound surface

- [x] 2.1 On `NotificationRequested` in Workspace, ring `AudibleBell` when sound prefs allow
- [x] 2.2 Ensure desktop_sound path still uses `UserNotification::new_with_sound`

## 3. Tests + evidence

- [x] 3.1 Unit tests: wait notify once; complete after wait; no duplicate wait
- [x] 3.2 `cargo test -p warp --lib cli_agent_monitor` (40 passed)

## 4. Workflow

- [x] 4.1 Save Grok workflow `cli-agent-human-gate-sound.rhai` under `.grok/workflows/`
