# Goal Spec: Herdr-class Agent Workspace en Warp

**Status:** draft (goal / product architecture)  
**Branch:** `feat/agent-task-hierarchy`  
**Fecha:** 2026-07-22  
**Referencia conceptual:** [Herdr](https://github.com/ogulcancelik/herdr) · [herdr.dev/docs](https://herdr.dev/docs/)  
**Licencia de Herdr:** AGPL-3.0-or-later — **inspiración de producto y arquitectura solamente**.  
**No copiar** código, manifests, UI pixel-perfect, ni textos de Herdr.

---

## 0. Una frase

Que Warp sea el lugar donde corrés **muchos coding agents a la vez** y, en **&lt;5 segundos**, ves quién trabaja, quién te necesita, quién terminó sin revisar, y saltás al **terminal real** — sin que Warp sea el runtime de IA ni un clon legal de Herdr.

---

## 1. Qué es Herdr (conocimiento de producto)

### 1.1 Producto

Herdr se describe como **agent multiplexer que vive en el terminal**:

| Idea | Qué hace en la práctica |
|------|-------------------------|
| **Multiplexor** | Server de fondo + clientes attach; panes = PTY reales |
| **Agent-aware** | Detecta agentes en panes y muestra estado en sidebar |
| **Glanceable** | Estados: `working` / `blocked` / `done` / `idle` / `unknown` |
| **Persistencia** | Detach sin matar procesos; restore de layout; resume nativo de sesión del agente |
| **Control programático** | CLI + socket API: spawn panes, leer output, wait entre agentes |
| **Mouse-first TUI** | Click, drag splits, menús; teclado opcional (prefix `ctrl+b`) |

Fuentes: [README](https://github.com/ogulcancelik/herdr), [concepts](https://herdr.dev/docs/concepts/), [agent guide](https://herdr.dev/agent-guide.md).

### 1.2 Modelo mental (orden de enseñanza de Herdr)

```
Session (server namespace)
  └── Workspace (proyecto / repo)
        └── Tab (layout: agents | logs | server…)
              └── Pane (PTY real)
                    └── Agent (proceso detectado + estado)
```

- **Sidebar** = dashboard: rollup de estados pane → tab → workspace.
- **Click en agente** = focus al pane real (no una UI inventada).
- **done** permanece hasta que el humano **lo ve** (attention/unseen).

### 1.3 Cómo clasifican estado (autoridad)

Herdr **no** confía en un solo truco:

1. **Proceso foreground** del pane (quién corre).
2. **Lifecycle hooks / integrations** (autoridad alta cuando están instaladas y reportan).
3. **Screen manifests** (TOML sobre bottom-buffer + OSC title/progress) cuando no hay lifecycle completo.
4. **Blocked es estricto**: solo UI visible de approval/question/permission; si no matchea → `idle` (no inventar blocked).

Codex/Claude en Herdr usan **screen manifest** para estado; integrations dan **session identity** para restore, no lifecycle completo.

Ver: [Agents](https://herdr.dev/docs/agents/), ejemplo de reglas Codex en su repo (`src/detect/manifests/codex.toml` — solo referencia; no copiar).

### 1.4 Persistencia (varios caminos, no uno solo)

| Camino | Procesos vivos | Layout | Pantalla | Conversación del agente |
|--------|----------------|--------|----------|-------------------------|
| Detach/reattach | Sí | Sí | Live | Sí (nunca paró) |
| Server restart | No | Snapshot | Solo con pane history (opt-in) | Solo native resume |
| Update `--handoff` | Best-effort | Sí | Live si handoff ok | Sí si proceso sobrevive |

Ver: [Session state](https://herdr.dev/docs/session-state/).

### 1.5 Principios de ingeniería (Herdr AGENTS.md)

Útiles como **patrones**, no como código:

- **State ≠ runtime** — datos puros testeables sin PTY.
- **Render puro** — no mutar estado al pintar.
- **Detección desacoplada** — lee snapshot de pantalla; no toca parser/viewport.
- **Server owns facts / client owns presentation** — metadata de agente en server; layout del sidebar en UI.
- **Evidence-based detection** — reglas AND/OR sobre regiones; no scrap del viewport scrolleado.

### 1.6 Lo que Herdr **no** es (y Warp tampoco debe fingir)

- No es el provider de LLM (no “es Claude”).
- No inventa jerarquía de subagents desde texto libre sin evidencia.
- No trata el scroll del usuario como fuente de estado del agente.

---

## 2. Qué ya tenemos en Warp (base real)

| Capacidad Herdr-like | En Warp hoy | Dónde |
|----------------------|-------------|--------|
| Navegación de agentes | ✅ rail `vertical_tabs` | Agents rail |
| Jerarquía CLI + subagents | ✅ Codex/Claude topology | `agent_tabs_projection`, CLI sessions |
| Focus al terminal real | ✅ | select tab → pane |
| Estados + badges ES | ✅ parcial | `agent_ops` + tab status |
| Attention strip | ✅ P0 | rail “Necesitan atención” |
| Ops multi-dim (exec/attention/runtime) | ✅ dominio | `app/src/workspace/agent_ops/` |
| Identidad (nombre/avatar/color) | ✅ | agent profiles |
| Launch providers New/Resume | ✅ | + menu / Settings |
| **In-session shell** (detail \| nav) | ✅ WIP polish | `codex_shell` / `codex_session_shell` |
| Historial durable subagents | ✅ | archive + disk persist |
| Timeline agrupada (no raw logs) | ✅ parcial | partition/group feed |
| Screen-manifest detection | ❌ | — |
| Server detach independiente de Warp | ❌ (Warp *es* el host) | N/A distinto |
| Socket API para que agentes se controlen | ❌ | P2 |
| Toast/sound “needs input / done” | ❌ parcial | P1 |
| Workspaces = proyecto con rollup multi-pane | ⚠️ distinto modelo | Warp workspace/tabs |

### Superficies Warp (no mezclar roles)

```
[ vertical_tabs = fleet / ops navigation ]
        │
        ▼ focus
[ terminal pane = agente real ]
        │
        └── si el pane es Codex con subagents:
            [ codex_shell = detail center | nav right ]
                 active + history + timeline
```

**Regla de producto:**  
- **Fleet** (muchos agentes / proyectos) → rail.  
- **Deep dive de un Codex + sus subagents** → shell in-session.  
- Nunca duplicar el mismo árbol en tres sitios con semánticas distintas.

Specs previos:

- `docs/specs/external-agent-monitor.md`
- `docs/specs/agent-operations-alert-center.md`
- `docs/specs/PRODUCTO-ACTUAL-monitor-ops.md`

---

## 3. Objetivo de producto (Warp)

### 3.1 Job to be done

> “Tengo 5 agentes (Codex, Claude, Grok…) en paralelo.  
> Necesito **ver y actuar** sin splittear la pantalla en 5,  
> sin Warp AI, y sin perder el terminal real.”

### 3.2 Experiencia objetivo (Herdr-class, Warp-native)

1. **Glance** — en el rail, colores/estados legibles: trabajando / esperando / bloqueado / error / terminado·sin revisar / offline.  
2. **Jump** — un click → pane correcto (o subagent detail en shell).  
3. **Act** — si blocked, el input del humano va al terminal real (o surface de approval si el provider la expone).  
4. **Review** — done no desaparece hasta “visto” / archivado con contenido útil.  
5. **History** — ejecuciones pasadas de subagents son buscables, copiables, borrables con confirmación.  
6. **Honestidad** — si no hay señal confiable, `unknown` / hoja simple; nunca árbol falso.

### 3.3 Principios (normativos)

1. **Terminal real > UI inventada** — el center por defecto es el PTY del agente padre.  
2. **Autoridad de señal explícita** — native &gt; integration &gt; process &gt; screen evidence &gt; heuristic.  
3. **Waiting ≠ Blocked** — colores y labels distintos (ya en shell; alinear rail + ops).  
4. **done + unseen** — “terminado sin revisar” es first-class (ops Attention).  
5. **State pure / render pure** — modelos de dominio testeables sin WarpUI.  
6. **AGPL fence** — reimplementar ideas; cero código/manifests de Herdr.

---

## 4. Mapa de capacidades: Herdr → Warp

| Capacidad Herdr | Meta en Warp | Fase | Notas |
|-----------------|--------------|------|-------|
| Sidebar agent states | Rail + badges + attention strip | **P0** | Ya base; unificar semántica con shell |
| Jump to pane | Select tab / subagent | **P0** | Ya |
| State rollup workspace | Badge del root agent = worst child | **P0** | Completar si falta |
| Blocked detection strict | Approval UI / plugin events / OSC | **P1** | Screen rules **propias**, no copiar TOML |
| Lifecycle / integration hooks | CLI plugins + JSONL / OSC ya en path | **P1** | Codex app-server / Claude events |
| Detach keeps agents | Warp window close policy / session restore | **P1** | Warp ya es host; no hace falta server Herdr |
| Native agent resume | Launch ResumeLast + session_id | **P0–P1** | Parcialmente hecho |
| Subagent hierarchy | Topology + in-session shell | **P0** | Codex shell WIP |
| Durable history | Archive validation + disk | **P0** | Hecho; UX polish |
| Timeline organized | group/partition feed | **P0** | Polish UI |
| Notifications toast/sound | Agent toast + OS notif | **P1** | `agent_toast` existe en codebase |
| Configurable sidebar rows | Settings tokens (later) | **P2** | No bloquear MVP |
| Socket API / agent skill | Local control surface | **P2** | Solo si hay use-case dogfood |
| Worktrees from sidebar | Warp worktree flows | **P2** | Ya hay worktree UI parcial |
| Plugins marketplace | Out | — | No |
| Full TUI multiplexer | Out | — | Warp es GUI desktop |

---

## 5. Alcance por fases

### P0 — “Herdr glance inside Warp” (dogfood diario)

**Outcome:** podés operar multi-agente sin splits mentales.

| # | Entrega | Done when |
|---|---------|-----------|
| P0.1 | **Semántica única de estados** en rail + shell + ops | Mismos labels ES y colores: Working blue, Waiting yellow, Blocked/Failed red, Completed green, Unseen completed = “terminado · sin revisar” |
| P0.2 | **Rollup** padre refleja peor hijo activo | Parent badge = max(urgency) de children |
| P0.3 | **Shell in-session** usable y limpio | Nav cards + detail sections (ya polish); empty states honestos; filtros history sin muro |
| P0.4 | **done → history** con validación | No hollow templates; persist-before-drop; retry archive |
| P0.5 | **Jump paths** | Rail → pane; shell nav → detail o parent terminal; attention strip → agente |
| P0.6 | **Launch** New/Resume fiable desde + | Labels visibles; resume con session id cuando exista |
| P0.7 | Tests dominio | nextest pure models verdes; smoke WarpOss |

**Out of P0:** screen manifests, socket API, sound, remote handoff.

### P1 — “Detection & attention quality”

| # | Entrega | Done when |
|---|---------|-----------|
| P1.1 | **Screen evidence layer** (Warp-owned) | Bottom-buffer / OSC title rules **propias** por agent id; blocked solo con evidencia visible |
| P1.2 | **Authority merge** | Integration hooks pisan heuristic; documentado en `agent_ops` |
| P1.3 | **Notifications** | Toast (o system) cuando blocked/done en pane no enfocado |
| P1.4 | **Search history free-text** | Input real en shell (no solo action stub) |
| P1.5 | **Nav resize** | Drag o preferencia de ancho del nav column |
| P1.6 | **Re-run similar** | Acción opcional desde history (prompt copy / re-launch) |

### P2 — “Orchestration surface”

| # | Entrega |
|---|---------|
| P2.1 | API/local control mínima (spawn pane, list agents, focus) — solo si dogfood lo exige |
| P2.2 | Multi-workspace project grouping con rollup |
| P2.3 | Runtime SSH/Daytona reporting real (hoy stub) |
| P2.4 | Configurable row layout en rail (inspiración tokens, implementación propia) |

---

## 6. Cómo se presenta cada agente (conocimiento de producto)

Fuente de verdad en código: `app/src/workspace/agent_presentation.rs` (`AgentUiProfile`).

| Provider | Superficie | Jerarquía | Resume | Shell deep-dive | Copy / nouns |
|----------|------------|-----------|--------|-----------------|--------------|
| **Codex** | Fleet + shell | JSONL `sub_agent_activity` | `codex resume --last` | Sí | subagents · sección `CODEX` · “← Terminal Codex” |
| **Claude Code** | Fleet + shell | tool_use / Task topology | `claude --continue` | Sí | tareas · `CLAUDE` · permisos frecuentes |
| **Grok** | Fleet leaf only | Ninguna confiable | No | **No** (no inventar hijos) | hoja honesta · focus PTY |
| Gemini / Kimi / MiniMax / … | Fleet leaf | No | Variable | No | mismos patterns leaf |

### 6.1 Cómo lo ve el usuario

**Codex / Claude (con hijos o historial):**

```
┌──── center (PTY padre | detail hijo) ────┬── nav ──────────────┐
│ Header + badge status                    │ CODEX / CLAUDE      │
│ Objetivo / Actividad / Archivos / Result │ Sesión principal    │
│ [← Terminal Codex] [cerrar] …            │ blurb capacidades   │
│                                          │ ACTIVOS · N         │
│                                          │ · child · status    │
│                                          │ HISTORIAL · M       │
└──────────────────────────────────────────┴─────────────────────┘
```

**Grok (y hojas):**

```
Rail:  Grok · trabajando|idle
Center: terminal real únicamente
// Sin columna nav de subagents — no hay topología que mostrar
```

### 6.2 Oportunidades (backlog agentic)

Tabla viva también en `agentic_ui_opportunities()`:

| Fase | Provider | Oportunidad |
|------|----------|-------------|
| **P0** | Codex | Shell branded + history + timeline limpia |
| **P0** | Claude | Misma shell, permisos → bloqueado, nouns “tareas” |
| **P0** | Grok | Leaf honesto; cero árbol fake |
| **P1** | Codex | Screen evidence blocked estricto (OSC / bottom-buffer **propios**) |
| **P1** | Claude | PermissionRequest/Question → attention strip |
| **P1** | Grok | Mejor estado de proceso sin inventar children |
| **P2** | Codex | Re-run similar desde historial |

### 6.3 Rail (fleet)

```
┌ Necesitan atención (2) ─────────────┐
│ · Codex · espera input              │
│ · Claude · bloqueado                │
├ Agentes ────────────────────────────┤
│ ▾ Codex main · TRABAJANDO           │
│    · refactor auth · trabajando     │
│    · tests · esperando              │
│ ▸ Claude · BLOQUEADO                │
│ ▸ Grok · idle                       │
└─────────────────────────────────────┘
```

Center con Parent selection = **terminal real del padre**, no un dashboard vacío.

---

## 7. Arquitectura objetivo en Warp

```
CLI / PTY / JSONL / OSC / plugins
        │
        ▼
Provider adapters (existing CLIAgentSessions + future screen_evidence)
        │  events with authority + sequence
        ▼
agent_ops store (Execution × Attention × Verification × Runtime)
        │
        ├──► AgentTabsProjection → vertical_tabs (fleet)
        │
        └──► CodexSessionShellState → codex_shell UI (deep dive)
                 ├── active rows (live)
                 └── history (durable disk)
```

### 7.1 Contratos

| Contrato | Owner | Testeable sin UI |
|----------|-------|------------------|
| `AgentOpsEvent` + authority | `agent_ops` | sí |
| `derive_visible_status` | `agent_ops/state` | sí |
| `reconcile_shell_membership` | `codex_session_shell` | sí |
| `archive` validation + persist | `codex_session_shell` | sí |
| Screen evidence match (P1) | nuevo módulo puro | sí |
| Render rail / shell | WarpUI | smoke + guías GUI |

### 7.2 Anti-patrones

- Scrapear todo el transcript como status authority.  
- Mezclar Waiting y Blocked en un solo “amarillo”.  
- Auto-borrar completed sin flash ni history.  
- Copiar manifests/UI de Herdr.  
- Meter UI de subagents en el rail **y** en el shell con datos divergentes.  
- Hacer de Warp un second multiplexer (server+detach) si no hay necesidad real — Warp ya es el host de PTYs.

---

## 8. Criterios de éxito (aceptación)

### Experiencia

- [ ] Con ≥3 agentes concurrentes, en &lt;5s identificás blocked/working/done-unseen sin abrir cada pane.  
- [ ] Click en blocked te deja en el terminal donde hay que responder.  
- [ ] Codex con subagents: shell aparece; Parent muestra terminal; Active muestra detail útil.  
- [ ] Completed con contenido real entra a historial y sobrevive restart de app.  
- [ ] Hollow “Completado: …” sin trabajo **no** contamina historial.  
- [ ] UI se siente intencional (cards, acento, pills) — no muro de chips.

### Técnico

- [ ] Modelos de dominio unit-tested (nextest).  
- [ ] Autoridad de eventos documentada y enforced.  
- [ ] Sin dependencia de Warp AI / créditos para el monitor.  
- [ ] Sin código AGPL de Herdr en el tree.

### No-regresión

- [ ] Terminal normal (sin CLI agent) no muestra chrome de shell.  
- [ ] Profiles / launch / onboarding existentes no se rompen.

---

## 9. Plan de trabajo sugerido (ejecución)

### Batch A — Cerrar P0 de shell + semántica (inmediato)

1. Congelar paleta/labels compartidos (rail + shell + ops).  
2. Rollup parent status desde children.  
3. Shell UX polish residual (search field, density, primary actions).  
4. Smoke E2E WarpOss con Codex multi-subagent.  
5. Checklist de §8 experiencia.

### Batch B — Attention quality (P1.1–P1.3)

1. Diseño de `screen_evidence` **propio** (regiones, AND/OR, blocked estricto).  
2. Wiring a `agent_ops` con authority Heuristic/Screen.  
3. Toasts para panes no enfocados.

### Batch C — History power (P1.4–P1.6)

1. TextInput search.  
2. Nav width.  
3. Re-run / copy prompt.

---

## 10. Riesgos

| Riesgo | Mitigación |
|--------|------------|
| Contagio AGPL | Solo docs propias; review legal mental: no paste de código/manifests |
| False blocked | Regla Herdr-like: blocked solo con evidencia de UI de approval |
| Dual UI divergente (rail vs shell) | Un solo `derive_visible_status` + tests de proyección |
| Scope creep a “multiplexor total” | Mantener Warp host; no server/client Herdr |
| Screen scrape frágil | Empezar por Codex/Claude; fallback idle honesto |

---

## 11. Glosario

| Término | Significado en este spec |
|---------|--------------------------|
| **Fleet** | Vista multi-agente del rail |
| **Deep dive** | Shell in-session de un padre + hijos |
| **Rollup** | Estado del contenedor = max urgencia de hijos |
| **Unseen done** | Completed + attention unseen |
| **Authority** | Quién manda en un update de estado |
| **Hollow history** | Entry sin objective/work/result reales |

---

## 12. Referencias

### Herdr (concepto)

- https://github.com/ogulcancelik/herdr  
- https://herdr.dev/docs/concepts/  
- https://herdr.dev/docs/agents/  
- https://herdr.dev/docs/session-state/  
- https://herdr.dev/agent-guide.md  

### Warp (base)

- `docs/specs/external-agent-monitor.md`  
- `docs/specs/agent-operations-alert-center.md`  
- `docs/specs/PRODUCTO-ACTUAL-monitor-ops.md`  
- `app/src/workspace/agent_ops/`  
- `app/src/workspace/codex_session_shell.rs`  
- `app/src/terminal/view/codex_shell.rs`  
- `app/src/workspace/view/vertical_tabs.rs`  

---

## 13. Decisión de producto (explicit)

| Pregunta | Respuesta |
|----------|-----------|
| ¿Copiamos Herdr? | **No.** Inspiración de workflow. |
| ¿Warp compite como TUI multiplexer? | **No.** Warp es GUI + terminal host. |
| ¿Warp AI es el centro? | **No.** Externos primero. |
| ¿Primera victoria medible? | **P0** glance + jump + shell + history honestos. |

---

**Next step recomendado:** ejecutar **Batch A** del §9 contra la checklist §8, luego abrir P1 screen_evidence con diseño propio (no port de manifests).
