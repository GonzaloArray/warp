## Approach

Pure domain in `MonitorStore::apply_live_signal`:

```
prev → next
if next == Waiting && !notified_waiting → queue HumanGate notification, set notified_waiting
if next == CompletedUnseen && !notified_completed → queue Completed notification, set notified_completed
```

UI layer (`CliAgentMonitorModel` + `Workspace`) already:

1. Sets pet sticky alert
2. Shows floating pet
3. Emits `NotificationRequested`
4. Workspace sends desktop `UserNotification` with `play_sound`

**Addition:** on `NotificationRequested`, if alerts.desktop_sound && SessionSettings.play_notification_sound → also `AudibleBell::ring()` so macOS always beeps even when Notification Center is muted.

## Titles (ES)

| Kind | Title | Body |
|------|-------|------|
| HumanGate | `{Agent} necesita tu atención` | `{project} — human-gate: esperando input` |
| Completed | `{Agent} terminó` | `{project} — pendiente de revisar` |

## Non-goals

- WhatsApp on every Waiting (optional later; keep WhatsApp on completed only unless prefs say otherwise).
- Grok Orchestra CLI (`orch-human-approve`) inside Warp — this is Warp observer UX only.
- Changing pet close semantics.

## Testing

- Unit: first Blocked → 1 pending human-gate; second Blocked → no duplicate; Success → completed notif independent; flags persist across re-apply same state.
