# Inventario de producto — Warp como monitor de agentes externos + ops center

**Fecha:** 2026-07-21  
**Rama:** `feat/agent-task-hierarchy`  
**Binario de prueba:** `target/debug/warp-oss` (build local, no release)  
**Estado Git:** trabajo en working tree (sin commit/push de este inventario salvo que se pida)

---

## En una frase

WarpOss en esta rama **no es un chat de Warp AI**: es un **monitor de CLIs externos** en el rail izquierdo (`vertical_tabs`), con **estados de ops**, **alertas**, **personalización de identidad**, **lanzamiento de providers** y **onboarding en español** con fondo Sumanos.

---

## 1. Superficie principal: rail Agents (`vertical_tabs`)

| Capacidad | Estado | Qué ves / qué hace |
|-----------|--------|---------------------|
| Rail de agentes siempre visible al lanzar (auto-reveal) | ✅ | Barra izquierda de Agents sin esconderse al abrir Warp |
| Cada CLI = fila/tab principal | ✅ | Claude, Codex, Gemini, Grok, Kimi, MiniMax, Hermes, OpenCode, Cursor, Copilot, etc. |
| Expandir / contraer jerarquía | ✅ | Space o chevron; no activa la terminal |
| Activar = terminal real | ✅ | Enter / click abre el pane/terminal real (no una UI inventada) |
| Teclado ↑/↓ solo filas visibles | ✅ | Sin wrap; selección se reconcilia al refrescar el árbol |
| Empty state en español | ✅ | Mensaje cuando no hay agentes |
| Botón **Edit** por agente | ✅ | Abre customizer de identidad local |
| Badges de estado en español (ops) | ✅ | TRABAJANDO, BLOQUEADO, ESPERA INPUT, TERMINADO · SIN REVISAR, DESCONECTADO, etc. |
| Strip “Necesitan atención” | ✅ | Contador + líneas urgentes + alertas abiertas; click → navega al agente |
| Goal en el árbol | ⚠️ parcial | Aparece **Agent → Goal → Task/Subagent** solo si el store ops tiene `goal_title` (p. ej. query/summary de la sesión CLI) |
| Subagents Codex / Claude | ✅ | Hijos estructurados desde topología del provider (JSONL / tool_use), no scrap de PTY |
| Oz / Warp AI en el rail | ⚠️ opcional | Puede coexistir; el producto prioriza externos |

**Principio:** el monitor **es** la navegación; no es una segunda pantalla.

---

## 2. Providers y launch

| Capacidad | Estado | Detalle |
|-----------|--------|---------|
| Detección CLI | ✅ | `CLIAgent`: Claude, Codex, Gemini, Grok, Kimi, MiniMax, Hermes, OpenCode, Cursor, Copilot, Amp, Droid, Pi, … |
| Hub de providers (Settings + menú) | ✅ | Enable/disable, menú de lanzamiento |
| Launch **Nueva sesión** / **Resume** | ✅ | Comandos útiles (no solo el binario pelado): p. ej. continue/resume según provider |
| Abrir rail al lanzar | ✅ | La sesión nueva cae en el monitor |
| Settings → Providers | ✅ | Configuración fuera del scroll de tabs |

---

## 3. Identidad del agente (customizer)

| Capacidad | Estado | Detalle |
|-----------|--------|---------|
| Nombre display | ✅ | Editable, se persiste en `agent-profiles.json` local |
| Avatar (Initial / Assistant / Code / Terminal) | ✅ | Allowlist (no upload de imagen arbitraria) |
| Color (Blue / Green / Orange / Purple / Red) | ✅ | Se aplica al avatar del rail |
| Feedback de selección | ✅ (fix reciente) | ✓ + tema Primary + **preview en vivo** |
| Modal con scrim | ✅ (fix reciente) | Fondo atenuado; click afuera cancela |
| Persistencia | ✅ | `config_local_dir()/agent-profiles.json` |

**Nota de color:** “Orange” se pinta con el amarillo ANSI del theme de Warp (mapeo de diseño existente).

---

## 4. Agent Operations (ops center P0)

**Código:** `app/src/workspace/agent_ops/`

| Módulo | Qué tiene |
|--------|-----------|
| `state` | Execution / Attention / Verification / Runtime; `derive_visible_status`, ranks de atención |
| `events` | Envelope con source, authority, sequence, confidence |
| `alerts` | Severidad, categoría, lifecycle, dedupe, navigation target, occurrence_count |
| `policy` | Escalamiento (umbral), supresión, expire (`tick_alerts`) |
| `store` | Snapshot + eventos ordenados; load/save JSON; aislamiento de dimensiones |
| `runtime` | Contrato `AgentRuntime`: Local / SSH / Daytona (stub de reporting) |
| `projection_bridge` | Une proyección del rail con ops store y strip de atención |

### Señales y autoridad

Prioridad: **nativo > orquestador > runtime > heurística > heartbeat**.  
Eventos viejos no pisan estado nuevo.  
`GoalProgress` **solo** toca campos de goal (no pone runtime Online).  
Tras disconnect, Offline se mantiene.

### Emisión desde CLI (live)

| Evento CLI | Qué hace en ops |
|------------|-----------------|
| Started | Runtime online |
| InProgress | Actividad / working (+ goal desde query/summary si hay) |
| Blocked / Failed / Completed | Estado + alerta (si no está suprimida) |
| Ended | Runtime disconnected (**sin** re-emitir GoalProgress) |

Autoridad actual de este path: **Heuristic** (observación de proceso/status), no JSONL nativo completo.

### Alertas

| Capacidad | Estado |
|-----------|--------|
| Crear / dedupe / storm (1000→1) | ✅ dominio + tests |
| Ack / resolve / dismiss / expire | ✅ dominio |
| Escalamiento por tiempo/ocurrencias | ✅ policy + tick en path CLI |
| Supresión (prompt visible, cleanup intencional, …) | ✅ policy; wired en CLI apply |
| UI lista en strip | ✅ |
| Panel multi-filtro (Nuevas / Ack / Historial) | ❌ no |
| Atajos “siguiente crítica / ack” | ❌ no |
| Slack / Telegram / email | ❌ no (y no se envían sin permiso) |

### Persistencia ops

| Artefacto | Path / forma |
|-----------|----------------|
| Store | `config_local_dir()/agent-ops-store.json` |
| Contenido | agentes (dims), `alerts_json`, cola de eventos (tail ~500) |
| Recuperación | load al crear Workspace; replay opcional `from_snapshot_and_events` |

---

## 5. Onboarding y branding

| Capacidad | Estado | Detalle |
|-----------|--------|---------|
| Textos onboarding en español (Rioplatense) | ✅ | Slides y callouts |
| Fondo Sumanos | ✅ | Solo **columna 2** del onboarding (no sidebar ni ventana entera) |
| Asset | ✅ | `app/assets/async/png/onboarding/onboarding_bg.png` (+ backup original) |

---

## 6. Qué **no** ofrece (todavía / a propósito)

- Orquestador propio tipo Sumanos cloud (provisioning multi-tenant, etc.).
- Inventar goals/tareas scrapeando el PTY.
- Centro de alertas completo tipo producto PagerDuty.
- Daytona production (solo adapter/stub de health reporting).
- Canales externos de notificación.
- Panel central Goal \| Diff \| Tests \| Evidencia \| Logs como IDE completo.
- “100% nativo” de eventos Codex/Claude JSONL en todos los providers.
- Sustituir Claude/Codex/etc. por Warp AI.

---

## 7. Cómo probarlo en la práctica

```bash
cd /Users/gonza/Desktop/warp
env -u COLORTERM cargo build --bin warp-oss --features gui
env -u COLORTERM ./target/debug/warp-oss
```

1. Abrir rail **Agents**.  
2. **+** → lanzar Claude/Codex (Nueva / Resume).  
3. Ver badge de estado y, si hay subagents, expandir.  
4. **Edit** → elegir color → **✓** + preview → **Save**.  
5. Forzar blocked/fail en el CLI → strip “Necesitan atención”.  
6. Onboarding (usuario nuevo / reset): columna derecha con foto Sumanos.

---

## 8. Documentos técnicos relacionados

| Doc | Contenido |
|-----|-----------|
| `docs/specs/external-agent-monitor.md` | Visión monitor de agentes externos |
| `docs/specs/agent-operations-alert-center.md` | Ops + alertas P0 |
| Este archivo | Inventario de lo **implementado hoy** |

Engram (SDD): change `agent-operations-alert-center` (explore → tasks → apply-progress).

---

## 9. Tests de respaldo (última corrida relevante)

| Suite | Resultado aproximado |
|-------|----------------------|
| `agent_ops` unit | 46 passed |
| projection + agent_monitor + vertical_tabs | ~110 passed |
| Customizer | feedback visual; validación manual en WarpOss |

---

## 10. Resumen para el usuario

**Tiene:** monitor de agentes externos en el rail, lanzamiento inteligente de CLIs, personalización de nombre/avatar/color, estados de ops en español, strip de alertas, jerarquía cuando hay datos estructurados, persistencia local de perfiles y ops, onboarding ES + fondo Sumanos en col2.

**No tiene (aún):** panel de alertas full, notificaciones externas, Daytona real, goals inventados sin señal estructurada, ni un producto “IDE de evidencia” completo.

**Cómo se usa hoy:** levantás WarpOss de este checkout, corrés tus CLIs, y el rail te dice **quién está trabajando, bloqueado o sin revisar** sin entrar a cada terminal.
