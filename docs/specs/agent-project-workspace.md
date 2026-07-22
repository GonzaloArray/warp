# Visión de producto: Agente como tab / accordion de trabajo

**Fecha:** 2026-07-21  
**Estado:** target (reemplaza el modelo “lista de procesos CLI”)  
**Branch:** `feat/agent-task-hierarchy`

---

## Problema con lo actual

Hoy el rail lista **procesos/sesiones** (Claude, Codex, “Agent task” Oz…) sueltos.  
El usuario ve `DESCONECTADO` y filas basura sin haber creado un “agente”.

## Modelo deseado

El **Agente** es la unidad de producto: un **tab/accordion** que el usuario crea.

```
+  →  Nuevo agente “Integrar Daytona”
       ▾ 🟣 Integrar Daytona          TRABAJANDO · 2 workers
            logo/avatar · Edit · menú config
            ▾ Goal / Tarea: runtime remoto     65%
                 ● Claude · implementando
                 ● Codex  · review
                 ● Grok   · research
            + Agregar tarea
            + Agregar worker (Claude / Codex / Grok / …)
```

### Sobre ese agente el usuario puede

| Acción | Detalle |
|--------|---------|
| **Crear** | Desde `+` del rail: nombre + (opcional) goal |
| **Logo / avatar / color / nombre** | Edit / customizer del **agente**: iconos built-in **o PNG/JPG propio** |
| **Avatar PNG** | File picker → copia a `config/agent-avatars/` → se ve en rail y preview |
| **Monitorear** | Estado rollup (quién trabaja, bloqueado, falló, sin revisar) |
| **Configurar el tab** | Proveedores habilitados, runtime default, preferencias del equipo |
| **Agregar workers** | Lanzar Claude/Codex/Grok… **colgados** de este agente |
| **Agregar tareas** | Nodos de trabajo bajo el agente; workers se asignan a tareas |
| **Abrir terminal** | Click en un worker → terminal real de ese CLI |
| **Trabajo en conjunto** | Todos los workers comparten el **contexto del agente** (cwd, goal, repo) |

### Qué **no** es el raíz del rail

- Un tab “New session” suelto  
- Un “Agent task” Oz desconectado  
- Un CLI huérfano sin padre (salvo vista “sin asignar” secundaria)

Los CLIs **vivos sin agente** pueden ir a un bucket opcional “Sin asignar”, no competir como si fueran el producto.

---

## Jerarquía de datos

```
AgentProject
  id, display_name, avatar, palette, goal, cwd, created_at
  ├── Task
  │     id, title, status, progress
  │     └── WorkerAssignment → CLI session / pane
  └── Worker (CLI)
        provider (claude|codex|grok|…), pane_id, session_id, status
```

- **AgentProject** = lo que el usuario “crea” y personaliza.  
- **Worker** = Claude/Codex/Grok corriendo (PTY real).  
- **Task** = unidad de trabajo; opcional en v1 si solo hay workers bajo el agente.

---

## UI del rail (target)

1. **Sin header “Agentes” rígido** como empty marketing; el rail **es** la lista de AgentProjects.  
2. **`+`** menú principal:
   - Nuevo agente  
   - (secundario) Nueva terminal  
   - Configurar proveedores  
3. Cada AgentProject:
   - Avatar + nombre + badge rollup  
   - Expand → tareas / workers  
   - Edit → logo, color, nombre  
   - Menú ⋮ → config, archivar, borrar  
4. **Sin banner “Necesitan atención” en el rail** — el rail es acordeón rápido de agentes/tareas; alertas van a un panel aparte (futuro), no arriba del árbol.

---

## Fases de implementación

| Fase | Entrega |
|------|---------|
| **A** | Crear AgentProject desde `+`; raíz en rail; Edit logo/nombre/color; persistencia |
| **B** | “Agregar worker” lanza CLI y lo cuelga del agente; click → terminal |
| **C** | Tareas bajo el agente; asignar worker a tarea |
| **D** | Rollup de estado + strip solo de AgentProjects; ocultar Oz “Agent task” basura |
| **E** | Config del tab (cwd, providers del equipo, defaults) |

---

## Relación con lo ya construido

| Reutilizar | Dejar de ser el centro |
|------------|-------------------------|
| Profile editor (logo/color/nombre) | CLI root como única identidad |
| agent_ops (estado, alertas, authority) | Mezclar Oz “Agent task” DESCONECTADO en el strip |
| Launch Claude/Codex | Launch sin padre AgentProject |
| Teclado árbol vertical_tabs | — |

---

## Criterio de éxito (usuario)

> “Creo un agente con +, le pongo logo y nombre, le sumo Claude y Codex, veo el accordion, monitoreo y configuro ese tab. No veo basura desconectada que no pedí.”
