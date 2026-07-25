## Why

En los loops de Orchestra/Grok, un **human-gate** (slice que necesita revisión humana) dispara **sonido + notificación**. En Warp, el monitor CLI del **pet** solo notifica al **completar** (`CompletedUnseen`). Cuando el agente está **bloqueado / esperando input** (`Waiting`), no hay alerta sonora: el usuario no se entera de que hay que intervenir.

## What Changes

- Human-gate: primera transición a `Waiting` (signal `Blocked`) → notification request (pet bubble + desktop + sound).
- Completado: se mantiene `CompletedUnseen` → notificación “terminó — revisar”.
- Sonido local confiable: además del flag `play_sound` del OS, **AudibleBell** (NSBeep / platform) al emitir la alerta.
- Prefs: `desktop_sound` / `desktop_enabled` siguen gobernando; no spam (flags `notified_waiting` / `notified_completed` por sesión).
- OpenSpec + tests unitarios del store; workflow Grok de apply/verify.

## Capabilities

### New Capabilities

- `cli-agent-human-gate-sound`: alertas de human-gate + completado con sonido en el monitor CLI/pet.

### Modified Capabilities

- Extiende el change `cli-agent-monitor-notif-ux` (pet + desktop notif) sin reemplazarlo.

## Impact

- `app/src/terminal/cli_agent_monitor/{store,session,model,alerts}.rs`
- `app/src/workspace/view.rs` (AudibleBell en `NotificationRequested`)
- Tests en `mod_tests.rs`
