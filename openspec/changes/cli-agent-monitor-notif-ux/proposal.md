## Why

El monitor CLI muestra “N agentes” inflado, la burbuja del pet no se puede cerrar, el click en la notificación/burbuja no trae Warp ni la sesión, y los agentes revisados no se pueden sacar de la lista. Eso rompe el flujo ADHD (ver → ir al terminal → limpiar).

## What Changes

- Contar solo agentes **activos** (no `Reviewed`) en el chip `N agentes`.
- Al **revisar** (click burbuja / notificación OS / panel), enfocar Warp + ventana + terminal (local o otra ventana).
- Permitir **descartar la burbuja** del pet sin marcar revisado (solo UI).
- Tras marcar **revisado**, **remover** la sesión del store (deja de contar y de listarse).
- Acción de panel: limpiar / marcar revisado de forma confiable.
- Click en notificación de macOS: traer la app al frente y activar la sesión correcta.

## Capabilities

### New Capabilities

- `cli-agent-notif-ux`: UX de notificaciones del pet + chip + navegación al terminal al ver una alerta.

### Modified Capabilities

- (ninguna spec existente en `openspec/specs/` para este dominio; el change `cli-agent-monitor` original vive en changes, no en specs principales)

## Impact

- `app/src/terminal/cli_agent_monitor/{store,model,ui,desktop_pet,panel_view}.rs`
- `app/src/workspace/view.rs` (activate + focus)
- `app/src/lib.rs` (notification click ya parcialmente cableado)
- Persistencia `~/.warp-oss/cli_agent_monitor/sessions.json`
