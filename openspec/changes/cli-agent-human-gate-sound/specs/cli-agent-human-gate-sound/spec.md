## ADDED Requirements

### Requirement: Human-gate notification on first Waiting

When a tracked CLI agent session first transitions into `Waiting` (from a `Blocked` live signal), the monitor SHALL queue exactly one human-gate notification for that session until the session is cleared or completed.

#### Scenario: First block notifies once

- **WHEN** session S is `Running`
- **AND** a live signal `Blocked` is applied
- **THEN** S is `Waiting` and exactly one pending notification with human-gate wording is queued
- **AND** a second `Blocked` while still `Waiting` does not queue another notification

#### Scenario: Human-gate wording identifies the AI

- **WHEN** a human-gate notification is queued for agent Codex
- **THEN** the title contains “Human-gate” and the AI name “Codex”
- **AND** the body includes `IA: Codex` plus project/cwd and a continue affordance (open session)
- **AND** the notification request carries `agent = Codex` so activate routes to that session

### Requirement: Completed notification remains

When a session first transitions into `CompletedUnseen`, the monitor SHALL queue a completion notification (existing behavior), independent of any prior human-gate notification.

#### Scenario: Complete after wait

- **WHEN** session S was notified for Waiting
- **AND** later a `Success` signal is applied
- **THEN** S is `CompletedUnseen` and a completion notification is queued

### Requirement: Sound on CLI agent monitor alerts

When the workspace handles `NotificationRequested` and desktop sound is enabled (alert prefs + session settings), Warp SHALL play an audible sound via the platform bell in addition to any OS notification sound flag.

#### Scenario: Desktop sound on

- **WHEN** `desktop_sound` is true and `play_notification_sound` is true
- **AND** a NotificationRequested is handled
- **THEN** AudibleBell rings (or equivalent platform beep)

### Requirement: No spam flags

Each session SHALL track `notified_waiting` and `notified_completed` so re-entry into the same alert kind does not re-notify until a new cycle warrants it (first transition only).
