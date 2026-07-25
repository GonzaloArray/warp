## Context

Persistencia en `~/.warp-oss/cli_agent_monitor/sessions.json`. Hoy hay 8 filas: IDs sintéticos `Codex:<entity>` + UUIDs de plugin, varios `reviewed` y algunos sin `terminal_view_id`. El chip usa `sessions.len()`. Activate solo busca terminal en la ventana local y no trae la app al frente.

## Goals / Non-Goals

**Goals**

- Chip refleja agentes vivos / pendientes de revisar (no basura).
- Click en burbuja o notificación OS enfoca Warp + terminal (o panel si no hay terminal).
- Se puede descartar la burbuja sin “revisar”.
- Tras revisar, la sesión sale del listado / deja de contar.

**Non-Goals**

- Borrar notificaciones del Notification Center de macOS (API limitada; el usuario las limpia en el sistema).
- Multi-pet o animaciones.

## Decisions

1. **Reconcile por terminal**: al trackear, colapsar filas con el mismo `terminal_view_id` o id sintético `Agent:entityId` hacia el `session_id` canónico del plugin.
2. **Conteo**: `active_agents` = no `Reviewed`; `pending_review` = `CompletedUnseen`.
3. **Post-review**: `remove_session` al pasar a `Reviewed` (ADHD: sticky solo hasta ver).
4. **Activate**: `show_window_and_focus_app` → focus local → focus otras ventanas → abrir panel monitor → `product_activate`. Si no hay terminal, abrir panel + mark reviewed al click explícito del usuario.
5. **Dismiss bubble**: limpia `active_alert` sin tocar el store.

## Risks / Trade-offs

- Remover al revisar pierde historial local de “revisados” (aceptable: el usuario quiere limpiar).
- Si entity ids se reciclan al reiniciar Warp, filas viejas se reconcilian o se marcan missing y se pueden limpia.

## Migration

Al cargar `sessions.json`, prune: drop `Reviewed`; merge duplicates by terminal_view_id keep newest non-synthetic id.
