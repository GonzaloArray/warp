## ADDED Requirements

### Requirement: Active agent count excludes reviewed sessions

The compact chip indicator SHALL count only non-`Reviewed` sessions as agents. Pending review count SHALL remain the number of `CompletedUnseen` sessions.

#### Scenario: Reviewed sessions do not inflate chip

- **WHEN** the store has 3 `Running` and 5 `Reviewed` sessions
- **THEN** the chip shows `3 agentes` (and pending only if any `CompletedUnseen`)

### Requirement: Canonical session per terminal

When tracking a live CLI session, the monitor SHALL keep a single row per terminal view. Synthetic ids (`AgentName:entityId`) SHALL be replaced by plugin session ids when available.

#### Scenario: Plugin id arrives after synthetic id

- **WHEN** a session was stored as `Codex:1646` for terminal `1646`
- **AND** a later event tracks UUID `019f…` for the same terminal
- **THEN** only the UUID row remains with `terminal_view_id = 1646`

### Requirement: Notification / pet click focuses Warp and session

Clicking the pet speech bubble or a system notification for a CLI agent SHALL bring Warp to the front, focus the associated terminal when it exists (any workspace window), open the monitor panel, and mark the session reviewed when focus succeeds or when the user explicitly opened a missing-terminal session from the alert.

#### Scenario: Focus succeeds

- **WHEN** the user clicks the pet alert for session S with a live terminal
- **THEN** Warp is focused, the terminal pane is focused, the alert clears, and S is removed from the monitor list as reviewed

#### Scenario: Terminal missing

- **WHEN** the user clicks the alert and no terminal exists
- **THEN** Warp is focused, the monitor panel opens, S is marked reviewed and removed, and the alert clears

### Requirement: Dismiss pet bubble without review

The user SHALL be able to dismiss the pet speech bubble without marking the session reviewed.

#### Scenario: Dismiss bubble

- **WHEN** the user dismisses the bubble (explicit dismiss control)
- **THEN** `active_alert` is cleared and the session state is unchanged

### Requirement: Clear reviewed / post-review removal

After a session becomes `Reviewed` via activate or explicit mark, it SHALL be removed from durable store so it no longer appears in the panel or chip.

#### Scenario: Mark reviewed removes row

- **WHEN** the user marks session S as reviewed
- **THEN** S is no longer in the store after persist
