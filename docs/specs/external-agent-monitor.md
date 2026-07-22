# Spec: Warp como monitor de agentes externos

**Status:** draft (reconstruido desde intent del usuario + sesión Codex `019f7d71-a34e-7b10-a4cf-f2a27395a261` + gap del código actual)  
**Branch:** `feat/agent-task-hierarchy`  
**Fecha:** 2026-07-21  
**Producto:** Warp (fork local / WarpOss) como **superficie de observación y navegación**, no como proveedor de IA.

---

## 1. Problema

Hoy Warp se usa como terminal con splits: para ver varios agentes hay que partir la pantalla (2 columnas, etc.). Eso no escala.

El usuario necesita:

1. Muchos agentes concurrentes (Codex, Claude Code, Grok, Kimi, MiniMax, …).
2. Ver **tareas y subagents** en un acordeón, no solo el pane activo.
3. **Personalizar** nombre, avatar/logo por “asistente”.
4. **Un solo lugar** de navegación: la barra izquierda.
5. **No depender de Warp AI** (ni créditos, ni plan, ni “New Agent Conversation” de Oz).

Cita de producto (sesión Codex, mensaje final del usuario):

> “la idea es armar agentes con estas tecnologías, no quiero warp ai”  
> Codex / Claude Code / Grok / Kimi / MiniMax

---

## 2. Objetivo de producto

Transformar la barra lateral izquierda (`vertical_tabs`) en un **monitor jerárquico de agentes externos**.

Cada **agente** es un tab principal expandible:

```
▾ Gonzalo · [avatar] · trabajando
   ▾ Tarea: Monitor gateway
      ● Arquitectura
      ! Seguridad
▸ Codex research · [avatar] · 2 tareas
▸ Claude Code · [avatar] · bloqueado
──────── tabs de terminal normales (opcional) ────────
```

### Principio rector

> El monitor no compite con los tabs.  
> **El monitor *es* la navegación principal.**

Warp observa, agrupa, muestra estado y navega a la terminal/detalle ya existente.  
Warp **no** es el runtime de IA ni inventa jerarquías leyendo texto libre del PTY.

---

## 3. Alcance

### In scope

| Área | Detalle |
|------|---------|
| Superficie UI | `vertical_tabs` (barra izquierda) |
| Fuentes | CLIs / harnesses externos con contrato de eventos |
| Jerarquía | Solo cuando el proveedor expone parent/children o equivalente estructurado |
| Identidad | Perfiles editables: display name, avatar/logo, color allowlisted |
| Interacción | Select ≠ expand; teclado árbol; estados visuales |
| Navegación | Focus del terminal/pane ya existente; no duplicar panes |
| Warp AI / Oz | Opcional, secundario; **no es prerequisito** |

### Out of scope (MVP)

- Inferir subagents scrapeando stdout del terminal.
- Scheduler / orquestador propio de Warp.
- Agrupar tareas solo por título/display name.
- Upload remoto de avatares / marketplace de perfiles.
- Reemplazar el runtime de Codex/Claude/etc.
- Forzar login a Warp AI / créditos para ver el monitor.

---

## 4. Actores y proveedores

### 4.1 Modelo mental

```
Usuario ejecuta agente externo (CLI / app)
        ↓ eventos estructurados
Adapter del proveedor (en Warp)
        ↓ AgentInstance + Task + Child (IDs estables)
AgentMonitorProjection (read-only)
        ↓
vertical_tabs (render + select/expand)
        ↓
Focus terminal / detalle existente
```

### 4.2 Niveles de fidelidad por proveedor

| Proveedor | Cómo se detecta hoy | Jerarquía confiable | MVP |
|-----------|---------------------|---------------------|-----|
| **Codex** | `CLIAgent::Codex` + eventos OSC/plugin / app-server | Alta si se consume topology de app-server / spawn metadata | **P0** — raíz + tasks/subagents cuando el contrato lo exponga |
| **Claude Code** | `CLIAgent::Claude` + rich plugin events | Media–alta vía JSONL / eventos de sesión | **P0** — raíz + hijos si hay IDs; si no, hoja con estado |
| **Grok** (CLI/runner) | `CLIAgent::Grok` + tokens `grok`/`xai` | Baja (sin topology de subagents) | **P0** — hoja nombrada + focus al terminal |
| **Kimi** | `CLIAgent::Kimi` + tokens `kimi`/`moonshot` | Baja | **P0** — hoja nombrada |
| **MiniMax** | `CLIAgent::MiniMax` + tokens `minimax`/`mini-max` | Baja | **P0** — hoja nombrada |
| **Hermes / Gemini / otros CLI** | Ya existen en `CLIAgent` | Variable | **P1/P2** — misma pipeline de adapters |
| **Warp Oz / Agent Mode** | `AgentConversationsModel` | Alta nativa | **P2 opcional** — no bloquear el MVP externo |

**Regla:** un proveedor sin topology confiable aparece como **hoja** (estado + focus al terminal). Nunca se fabrica un árbol falso.

---

## 5. UX requerida

### 5.1 Estructura visual

- Sección **Agents** arriba en el rail vertical.
- Cada raíz: chevron (si tiene hijos) · avatar · nombre personalizado · estado/resumen.
- Hijos indentados: tareas → subagents / alertas.
- Separador y tabs normales de terminal debajo (si el usuario los usa).
- Empty state honesto cuando no hay agentes externos activos:

  > **Agents**  
  > Run Codex, Claude Code, or another agent CLI — it will appear here.

### 5.2 Interacciones (invariantes)

| Acción | Resultado |
|--------|-----------|
| Clic en chevron / tecla ←→ | Solo expand/collapse; **no** cambia el tab activo |
| Clic en nombre/cuerpo de fila | Selecciona + enfoca terminal/detalle existente |
| Enter | Activa el destino |
| Space | Toggle expand sin activar |
| ↑/↓ | Mueve foco entre filas visibles |
| Sin destino resoluble | Mantiene selección; muestra “detalle no disponible”; **no** crea pane duplicado |

### 5.3 Estados (visuales)

| Estado | Significado |
|--------|-------------|
| Working | Turno/proceso en curso |
| Waiting | En cola / esperando input |
| Blocked | Permission / approval / human-in-the-loop |
| Completed | Turno o sesión exitosa |
| Failed | Error / cancelado con fallo |
| Unavailable | Sesión muerta / sin metadata |

Resumen colapsado (ejemplos): `trabajando`, `bloqueado`, `2 tareas`, `1 alert`.

### 5.4 Personalización

- Por **identidad estable del agente** (no por título de prompt):
  - display name
  - avatar/logo (bundled o inicial)
  - color allowlisted
- UI mínima de edición (context menu / “Customize…”).
- Fallback seguro si no hay perfil.

**Prohibido:** usar display name / prompt / command como clave de identidad o de agrupación.

---

## 6. Modelo de datos (especificación)

### 6.1 Identidades

```text
AgentInstanceId   = Provider + StableKey
TaskId            = ProviderTaskId | LocalOpaqueId
ChildId           = ProviderChildId | LocalOpaqueId
MonitorNodeId     = Instance | Task | Child  (typed, comparable)
```

- `StableKey` para CLI: preferir `session_id` del proveedor si es seguro; si no, `pane-{entity_id}` de por vida del pane.
- Nunca reparentar por texto.
- Nunca mergear dos instancias distintas solo porque el usuario les puso el mismo nombre.

### 6.2 Nodos

```text
AgentRoot
  display_name, avatar, status, summary, provider
  children: Task[]

Task
  title (display only), status, summary
  children: Child[]   // subagent, tool-run, alert — solo si el adapter lo emite

Child (Subagent | Alert | Process)
  label, status
  leaf (MVP)
```

### 6.3 Proyección

`AgentMonitorProjection` es **read-only**, se reconstruye por snapshot:

1. Cada adapter emite instancias + topology confiable.
2. Merge por `MonitorNodeId` (dedupe).
3. Orden: orden nativo del adapter / orden de aparición del pane; **nunca** sort por display name.
4. Reconciliación preserva: `expanded_ids`, selection, scroll/foco.

### 6.4 Adapters (contrato mínimo)

```text
trait ExternalAgentAdapter {
  provider: ProviderId
  detect(session) -> Option<AgentInstance>
  topology(instance) -> Tree<Task|Child>   // puede ser vacía
  status(instance) -> Status
  focus_target(node) -> Option<WorkspaceTarget>
}
```

**MVP adapters:**

1. **CodexAdapter** — eventos existentes + (fase siguiente) app-server topology / inter-agent metadata.
2. **ClaudeCodeAdapter** — rich plugin events + JSONL de sesión cuando esté disponible.
3. **GenericCliAdapter** — process/session leaf para Grok/Kimi/MiniMax/otros sin topology.

OzAdapter queda opcional y **no** es la fuente principal del producto.

---

## 7. Integración en Warp (architectura)

### 7.1 Superficie

- Única UI del monitor: `app/src/workspace/view/vertical_tabs.rs`.
- No reintroducir un segundo monitor en Agent Management bajo el flag de producto.
- Agent Management puede seguir existiendo para otras funciones; **no** duplica el árbol navegable.

### 7.2 Flujo

```
CLIAgentSessionsModel  ──┐
Codex topology feed    ──┼──► adapters ──► AgentMonitorProjection
Claude session feed    ──┤                    │
Profiles store         ──┘                    ▼
                                    vertical_tabs::render_agents
                                              │
                          WorkspaceAction::{Select, Expand, FocusTerminal…}
```

### 7.3 Feature / rollout

- Producto default-on en el fork dogfood (WarpOss) cuando el rail vertical está disponible.
- No esconder el monitor detrás de “tenés que crear un Agent Mode de Warp”.
- Auto-abrir el rail al detectar el **primer** agente externo activo (una sola vez por workspace; respetar cierre manual después).

### 7.4 Gap vs implementación actual (2026-07-21)

| Actual | Spec |
|--------|------|
| Proyección centrada en Oz hierarchy | Centrada en **CLIs externos** |
| Externos: solo **Codex + Hermes**, siempre **hoja** | Codex + Claude P0; Grok/Kimi/MiniMax hoja; hierarchy cuando exista contrato |
| Empty state asume “agent conversations” (Oz) | Empty state habla de **CLIs externos** |
| Validación visual atada a Warp AI / login | Validación con `codex` / `claude` reales en terminal |
| Subagents solo si Oz spawnea | Subagents solo si el **proveedor externo** expone hijos |

El trabajo en `feat/agent-task-hierarchy` **reutiliza** rail, select≠expand, perfiles y proyección; hay que **reorientar la fuente de verdad**.

---

## 8. Requisitos (normativos)

### R1 — Navegación primaria
GIVEN un agente externo activo  
WHEN el usuario abre el rail  
THEN ese agente aparece como fila principal en Agents, por encima de tabs normales.

### R2 — Select vs expand
GIVEN una raíz con hijos  
WHEN el usuario hace clic en el chevron  
THEN se expande/colapsa y **no** se cambia el pane activo.  
WHEN el usuario hace clic en el nombre  
THEN se enfoca el terminal/detalle del agente.

### R3 — Sin Warp AI
GIVEN el usuario no tiene plan/créditos/API de Warp AI  
WHEN corre `codex` o `claude` en un terminal de Warp  
THEN el monitor igual muestra la sesión.

### R4 — Sin topología inventada
GIVEN un proveedor sin parent/child estructurado  
WHEN la sesión está activa  
THEN se muestra como hoja con estado; **no** se crean hijos a partir del texto del terminal.

### R5 — Codex hierarchy (cuando el contrato exista)
GIVEN Codex reporta spawns / subagents con IDs  
WHEN el adapter los recibe  
THEN aparecen bajo la raíz/task correspondiente con estados honestos.

### R6 — Claude Code
GIVEN una sesión Claude Code detectada  
WHEN hay eventos de session/stop/permission  
THEN la raíz refleja Working/Blocked/Completed; hijos solo si el feed trae IDs.

### R7 — Grok / Kimi / MiniMax
GIVEN el CLI está en primer plano y es reconocible  
THEN aparece como hoja con nombre de proveedor + estado de proceso/sesión.

### R8 — Perfiles
GIVEN el usuario personaliza nombre/avatar de una identidad estable  
WHEN la sesión se reabre con la misma key  
THEN se reaplican los metadatos de presentación.

### R9 — Empty state
GIVEN cero agentes detectados  
THEN el rail muestra empty state de Agents y tabs normales siguen usables.

### R10 — Accesibilidad
GIVEN el monitor tiene foco  
THEN cada fila expone rol de árbol, expanded/selected, label sanitizado (sin prompts crudos ni paths sensibles).

---

## 9. Plan de entrega (fases)

### Fase 0 — Alinear producto (esta spec)
- [x] Congelar intent: Warp = monitor, no proveedor.
- [ ] Invalidar “MVP = Oz hierarchy” como éxito de producto.
- [ ] Aceptar esta spec.

### Fase 1 — Rail externo usable (MVP visible)
1. Incluir en proyección **todos** los `CLIAgent` relevantes (mínimo: Claude, Codex, Gemini, Hermes; + mapping configurable para Grok/Kimi/MiniMax).
2. Empty state orientado a CLIs.
3. Auto-reveal del rail al primer CLI detectado.
4. Select → focus terminal; expand no-op si no hay hijos.
5. Smoke: abrir WarpOss, correr `codex` / `claude`, ver filas reales **sin login Warp AI**.
6. **Providers hub** en el rail: On/Off, ready/missing/active, Launch → nueva terminal con el CLI (`agent_provider_hub.rs`).

### Fase 2 — Codex hierarchy real
1. ~~Consumir topology confiable~~ — `parse_codex_subagent_topology` lee `sub_agent_activity` del rollout JSONL (`~/.codex/sessions/**/rollout-*-{session_id}.jsonl`).
2. ~~Mapear a Task/Subagent~~ — `MonitorNodeId::ExternalChild` + auto-expand + guías de árbol en UI.
3. ~~Tests unitarios del adapter + proyección~~ — 24 tests verdes.
4. Smoke: sesión Codex con subagents visibles en acordeón (pendiente visual WarpOss).

### Fase 3 — Claude Code hierarchy
1. Adapter sobre eventos JSONL / plugin.
2. Blocked = permission request, etc.
3. Smoke con subagents si el feed lo permite; si no, hoja honesta.

### Fase 4 — Grok / Kimi / MiniMax
1. Detección de proceso/comando.
2. Hoja + estado; pluggable topology si más adelante hay contrato.
3. Perfiles por provider+key.

### Fase 5 — Polish
- Perfil editor accesible desde el rail.
- Resumen colapsado rico (conteos honestos).
- Persistencia de expansión.
- Quitar dependencia residual de Oz para demo del producto.

---

## 10. Criterios de aceptación (Definition of Done)

El feature está “listo para uso real” cuando:

1. **Sin Warp AI:** con solo CLIs externos, el rail muestra agentes.
2. **Codex y Claude Code** aparecen como tabs principales al estar activos.
3. **Select ≠ expand** se cumple en mouse y teclado.
4. **Al menos un proveedor** (Codex preferido) muestra acordeón real de tasks/subagents con datos estructurados.
5. Grok/Kimi/MiniMax (o el CLI que el usuario use) aparecen al menos como hojas con estado.
6. Perfil nombre/avatar se puede setear y sobrevive reabrir la sesión (misma identity key).
7. Tests unitarios de proyección/adapters en verde.
8. Smoke visual documentado con capturas en WarpOss (no solo tests unitarios).

**No** cuenta como done: empty state bonito + tests de Oz sin un CLI externo real en pantalla.

---

## 11. Riesgos y decisiones

| Riesgo | Mitigación |
|--------|------------|
| Codex no expone children en el cliente actual | Fase 1 hoja; Fase 2 solo con contrato; nunca scrapear texto |
| Cada CLI tiene formato distinto | Adapters pluggables; GenericCliAdapter de fallback |
| Usuario agrupa mentalmente por “Asistente Gonzalo” varias tareas | Requiere identity estable o asignación explícita futura; no heurística de título en MVP |
| Confundir monitor con Agent Management | Una sola superficie: vertical_tabs |
| Dependencia de Xcode/bundle | Validar con WarpOss local; no bloquear spec por tooling |

### Decisiones cerradas

1. **Warp AI no es prerequisito.**
2. **No scrapear terminal para inventar árbol.**
3. **vertical_tabs es la única UI del monitor.**
4. **Select y expand son acciones distintas.**
5. **Identidad ≠ display name.**

### Decisiones cerradas (2026-07-21)

6. **MVP hierarchy (acordeón con hijos):** solo **Codex** cuando el adapter tenga topology estructurada (Fase 2).
7. **Claude Code en el mismo release de Fase 1:** raíz/hoja con estado (Working/Blocked/Completed); hijos cuando el feed lo permita (Fase 3), no bloquea el MVP de filas visibles.
8. **Grok / Kimi / MiniMax:** hoja vía `ExternalProvider::Other` / detección CLI hasta que exista contrato.

---

## 12. Trazabilidad

| Fuente | Aporte |
|--------|--------|
| Codex session `019f7d71-…` | Intent UX (acordeón, subagents, personalización), pivot “no Warp AI”, proveedores externos |
| Goal del usuario en sesión | Estructura visual y 10 comportamientos |
| Código `agent_tabs_projection.rs` | Base reutilizable + gap (solo Codex/Hermes hoja; Oz-first) |
| `CLIAgent` enum | Superficie real de detección de CLIs en Warp |

---

## 13. Próximo paso de implementación (después de aceptar spec)

1. Reescribir proyección: **externos primero**, Oz opcional.
2. Ampliar `ExternalProvider` / adapters más allá de Codex+Hermes.
3. Empty state + auto-reveal sin Oz.
4. Smoke con `codex` real en WarpOss.
5. Luego hierarchy Codex (Fase 2).
