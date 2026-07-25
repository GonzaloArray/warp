## ADDED Requirements

### Requirement: Monitor session state machine
The system SHALL track each monitored CLI agent session with exactly one of: `RUNNING`, `WAITING`, `COMPLETED_UNSEEN`, `REVIEWED`, `ERROR`, `UNKNOWN`.

#### Scenario: Finished becomes completed unseen
- **WHEN** a live session transitions to a finished/success signal
- **THEN** the monitor state MUST become `COMPLETED_UNSEEN` and the pending-review count MUST increase by one (if it was not already `COMPLETED_UNSEEN` or `REVIEWED`)

#### Scenario: Waiting for user input
- **WHEN** a session is blocked on permission or confirmation
- **THEN** the monitor state MUST be `WAITING`

#### Scenario: Failure
- **WHEN** a session fails or is stuck in a hard error
- **THEN** the monitor state MUST be `ERROR`

### Requirement: State priority ordering
The system SHALL order sessions for display by priority: `ERROR` > `WAITING` > `COMPLETED_UNSEEN` > `RUNNING` > `REVIEWED` > `UNKNOWN`.

#### Scenario: Mixed states sort
- **WHEN** sessions exist in ERROR, RUNNING, and COMPLETED_UNSEEN
- **THEN** ERROR appears before COMPLETED_UNSEEN, which appears before RUNNING

### Requirement: ADHD review persistence
The system MUST persist `COMPLETED_UNSEEN` sessions so they remain green / pending after process death, terminal close, app restart, and panel close. Dismissing a notification, hovering, or elapsed time alone MUST NOT mark `REVIEWED`.

#### Scenario: Reload after restart
- **WHEN** the store is reloaded from durable storage after a finished session was saved as `COMPLETED_UNSEEN`
- **THEN** that session MUST still be `COMPLETED_UNSEEN` with pending review true

#### Scenario: Non-review interactions
- **WHEN** the user dismisses a desktop notification, hovers a row, or waits without focusing
- **THEN** the session MUST remain `COMPLETED_UNSEEN`

### Requirement: Review only after successful navigation or explicit mark
The system MUST set `REVIEWED` only when (1) the user activates a session and navigation to the associated terminal pane succeeds, or (2) the terminal no longer exists and the user chooses “Marcar como revisado”. The system MUST NOT auto-discard finished sessions.

#### Scenario: Successful focus reviews
- **WHEN** the user clicks a session whose terminal still exists and focus succeeds
- **THEN** the session state MUST become `REVIEWED` and pending-review MUST clear for that session

#### Scenario: Missing terminal requires explicit mark
- **WHEN** the user clicks a session whose terminal is gone
- **THEN** the system MUST keep showing the persisted summary and only mark `REVIEWED` after an explicit “Marcar como revisado” action

#### Scenario: Failed focus does not review
- **WHEN** navigation is attempted but focus fails
- **THEN** the session MUST remain not-reviewed (`COMPLETED_UNSEEN` or prior state)

### Requirement: Detection adapters for Claude, Codex, and Grok
The system SHALL detect Claude Code, Codex CLI, and Grok CLI via a common adapter interface using process/command signals (name, tree, cwd, TTY when available) and existing Warp session events—not window title scraping alone. Multiple same-type agents and multiple sessions in one repo MUST remain distinct. New agent types MUST plug in via the common adapter.

#### Scenario: Claude detection
- **WHEN** process/command fixtures match Claude Code
- **THEN** the adapter MUST identify agent kind Claude

#### Scenario: Codex detection
- **WHEN** process/command fixtures match Codex CLI
- **THEN** the adapter MUST identify agent kind Codex

#### Scenario: Grok detection
- **WHEN** process/command fixtures match Grok CLI
- **THEN** the adapter MUST identify agent kind Grok

#### Scenario: Multi-instance disambiguation
- **WHEN** two Claude processes run in different terminals or cwds
- **THEN** the monitor MUST track two distinct sessions

#### Scenario: Extra adapter registration
- **WHEN** a fake agent type is registered on the common adapter registry
- **THEN** matching fixtures MUST be detected as that agent type

### Requirement: Compact bar and panel UI
The Warp header toolbar MUST show a compact indicator of the form `[icon] N agentes · M para revisar`. Clicking MUST open a single-column panel listing sessions with agent, project/directory, state, elapsed time, and review-pending. Colors MUST map: green=`COMPLETED_UNSEEN`, yellow=`WAITING`, red=`ERROR`, neutral animated=`RUNNING`, gray=`REVIEWED`.

#### Scenario: Indicator copy
- **WHEN** there are 3 sessions and 2 pending review
- **THEN** the indicator text MUST include `3 agentes` and `2 para revisar`

#### Scenario: Panel row fields
- **WHEN** the panel is open
- **THEN** each row MUST expose agent, project/directory, state, elapsed, and review-pending

### Requirement: Finish notification
When a session first enters `COMPLETED_UNSEEN`, the system MUST request a desktop notification. Closing the notification MUST NOT mark the session reviewed.

#### Scenario: Notify on complete
- **WHEN** state transitions to `COMPLETED_UNSEEN` for the first time
- **THEN** a notification request MUST be recorded/emitted by the monitor domain

### Requirement: Navigation without duplicates
Activating an active session MUST focus the associated existing Warp pane (not open a duplicate terminal). Association uses terminal view / pane identity from session tracking.

#### Scenario: Focus existing pane
- **WHEN** the user activates a session with a live terminal association
- **THEN** the navigation path MUST request focus of that pane id and MUST NOT request creating a new terminal for that activation
