# Goal: Herdr-class **nativo** en Warp

**Status:** active goal (north star)  
**Branch:** `feat/agent-task-hierarchy`  
**Fecha:** 2026-07-24  
**Owner:** Gonzalo (fork)

**Referencias (inspiración de producto, no clon de código):**

| Fuente | Rol |
|--------|-----|
| [Herdr](https://github.com/ogulcancelik/herdr) · [herdr.dev](https://herdr.dev) | Agent multiplexer: glance + PTY real + detach + API |
| [Video: Herdr hace que Claude/Cursor/Grok trabajen en equipo](https://www.youtube.com/watch?v=Bu1kOdrXmoc) | Feeling de flota: varios agents, un tablero, foco al que te necesita |
| Warp Agent Mode + panes/tabs | Runtime y shell **ya nativos** (no hace falta un segundo binario) |
| Specs hijas | `herdr-class-agent-workspace.md`, `external-agent-monitor.md`, `fleet-room.md`, `GOAL-multiagent-loop-terminal.md` |

**Licencia Herdr:** Apache-2.0 (repo actual). Aun así: **no copiar** código, manifests, UI pixel-perfect ni textos. Solo el **modelo mental** y el outcome de producto.

---

## 0. Una frase

Que Warp sea el **multiplexor de agents nativo**: en &lt;5 s ves **toda la flota** (Claude / Codex / Grok / Agent Mode / CLI), sabés **quién trabaja / te bloquea / terminó**, **saltás al pane real** (PTY o conversación Agent Mode), y podés **orquestar** sin salir de Warp — como Herdr, pero **dentro** del terminal que ya es Warp.

---

## 1. Qué es Herdr (producto, no código)

Herdr se vende como **agent multiplexer que vive en el terminal** (tmux para coding agents):

| Capacidad Herdr | Qué significa para el usuario |
|-----------------|-------------------------------|
| **Glanceable fleet** | Sidebar: blocked / working / done / idle por agent |
| **Real panes** | Cada agent corre en un **PTY real**, no un chat simulado |
| **Workspace → tab → pane** | Jerarquía de layout por proyecto |
| **Detach / reattach** | Los processes siguen; volvés desde otro TTY o SSH |
| **Detección de estado** | Foreground process + screen rules + hooks (no un solo truco) |
| **Socket/CLI API** | Otros agents pueden spawn panes, leer output, esperar estados |
| **Mouse + teclado** | Click al agent = focus al pane; splits drag |
| **Soporta** | Claude Code, Codex, OpenCode, Amp, Droid, Cursor CLI, etc. |

**Por qué importa el video:** no es “otro chat multi-modelo”. Es **equipo de workers en terminales**, con un **tablero de control** que no te hace perder el hilo.

---

## 2. Por qué Warp puede ser Herdr-class **nativo**

Warp **ya es** más que un TTY:

| Capa | Warp ya tiene | Herdr equivalente |
|------|---------------|-------------------|
| Terminal / panes / tabs | Workspace, pane groups, splits | Workspace / tab / pane |
| PTY real | TerminalModel, local shell | Pane PTY |
| Agent Mode nativo | Oz / blocklist / LLM providers | (Herdr no tiene inferencia propia) |
| CLI agents | `CLIAgent` + OSC/plugins | Detección de Claude/Codex/… |
| Monitor | Vertical tabs + topology + voice | Sidebar glance |
| Notificaciones | Toasts / agent notifications | Sound/toast |
| Multi-modelo nativo | Claude/Codex/Grok vía Agent Mode + BYOK/Grok OAuth | N/A (Herdr solo multiplexa CLIs) |

**Ventaja Warp vs Herdr:** un solo producto puede unir:

1. **Runtime A** — Agent Mode nativo (inferencia Warp/BYOK, switch Claude/Codex/Grok).  
2. **Runtime B** — CLIs en panes reales (lo que Herdr hace 100%).  
3. **Control plane** — un solo rail/fleet que entiende **ambos**.

Herdr es multiplexor puro. **Warp-native Herdr-class** = multiplexor + Super Agent nativo en el mismo glass.

---

## 3. Qué NO es este goal

| Anti-goal | Por qué |
|-----------|---------|
| Clonar Herdr (UI, código, socket wire) | Legal + inútil: Warp no es un TUI binario embebido |
| Reemplazar Warp por un server tipo Herdr | Warp ya es el host de sesión |
| Solo “Local Agents” chat tipo BLOOME | Eso es un entrypoint; el goal es la **flota + focus** |
| Que Warp sea el runtime de Claude/Codex CLI | Los CLIs siguen siendo dueños de su proceso |
| Feature soup sin glance &lt;5 s | Si no triagás la flota, no es Herdr-class |

---

## 4. Modelo mental nativo (map Herdr → Warp)

```
Warp Window
  └── Workspace (repo / cwd)
        └── Tabs / Pane groups          ← layout (ya existe)
              ├── Pane: Agent Mode      ← Super Agent nativo (Claude/Codex/Grok switch)
              ├── Pane: PTY + `claude`  ← CLI real
              ├── Pane: PTY + `codex`
              └── Pane: PTY + `grok`
        └── Fleet rail (control)        ← Herdr sidebar nativo
              working / blocked / done / need-you
              click → focus pane
```

**Regla de oro (igual que Herdr):**  
**State ≠ runtime.** El rail **observa** y **enfoca**. No reinterpreta el transcript como “verdad” del agent salvo contratos estructurados (JSONL, tool_use, Agent Mode events).

---

## 5. Arquitectura objetivo (3 planos)

### A. Runtime (no reinventar)

| Tipo | Cómo corre en Warp |
|------|--------------------|
| **Native seat** | Agent Mode + `LLMPreferences` (Anthropic / OpenAI-Codex / xAI-Grok / Google) |
| **CLI seat** | Tab terminal + one-click launch (`claude` / `codex` / `grok`) |
| **Subagents** | Topology de Codex JSONL / Claude Agent·Task; voice layer |

### B. Control plane (el “Herdr” dentro de Warp)

| Superficie | Job |
|------------|-----|
| **Fleet rail** | Lista glanceable de **todos** los seats (native + CLI) |
| **Status model** | `working` / `blocked` / `waiting` / `done` / `failed` / `idle` |
| **Focus** | Click → pane/tab real (PTY o Agent Mode) |
| **Rollup** | Peor estado del subárbol (parent subagents) |
| **Toasts** | Need-you / done sin revisar |
| **Super Agent hub** | Entry multi-connector + switch de modelo nativo |

### C. Orchestration (opcional, P2)

| Capacidad Herdr | Warp-native analog |
|-----------------|--------------------|
| Socket API spawn/wait | Warp CLI / actions internas / Agent Mode tools (no copiar socket) |
| Agents spawn agents | Ya existe StartAgent / CLI subagents; el rail debe **mostrarlos** |
| Detach reattach | Warp sessions + cloud/shared (distinto de herdr server; no forzar 1:1) |

---

## 6. Outcomes medibles (Definition of Done)

### P0 — Glance + focus (Herdr core)

| # | Outcome | Baseline fork (2026-07-24) |
|---|---------|----------------------------|
| P0.1 | Rail muestra CLI sessions + status glanceable | ✅ parcial (topology + ops) |
| P0.2 | Subagents Codex/Claude con voz legible | ✅ voice layer |
| P0.3 | Click rail → focus al terminal/agent real | ⬜ parcial |
| P0.4 | One-click launch Claude/Codex/Grok (CLI **o** native) | ✅ hub + Super Agent nativo (en progreso) |
| P0.5 | Dogfood: 3 agents en paralelo, triage &lt;5 s sin adivinar tabs | ⬜ |

### P1 — Flota unificada (native + CLI en un glass)

| # | Outcome | Estado |
|---|---------|--------|
| P1.1 | Un rail unifica **Agent Mode seats** y **CLI seats** | ⬜ |
| P1.2 | Super Agent: 1 conversación + switch Claude/Codex/Grok nativo | ✅ código; dogfood |
| P1.3 | Blocked/need-you con evidencia (permisos CLI / questions Agent Mode) | ⬜ parcial |
| P1.4 | Layout “workspace agent”: split agent + terminal sin perder fleet | ⬜ |

### P2 — Multiplexor serio

| # | Outcome | Estado |
|---|---------|--------|
| P2.1 | Persistencia de layout multi-agent al reabrir Warp | ⬜ |
| P2.2 | API/acciones para spawn agent desde otro agent (control plane) | ⬜ |
| P2.3 | Notificaciones OS cuando blocked/done (Herdr-class alerts) | ⬜ parcial toasts |
| P2.4 | Obsidian / vault del loop (runs + estados) | ⬜ |

### P3 — Polish de rebaño

| # | Outcome |
|---|---------|
| P3.1 | Screen-evidence adapters para CLIs sin topology |
| P3.2 | Fleet empty-state + onboarding “tu rebaño” |
| P3.3 | E2E dogfood script (3 native + 2 CLI) |

---

## 7. Mapa Herdr feature → Warp implementation

| Herdr | Warp nativo (dónde vive) | Notas |
|-------|--------------------------|--------|
| Sidebar fleet | Vertical tabs / agent monitor rail | Completar focus + unify native |
| Pane = PTY | `TerminalView` + pane group | Ya |
| Agent detection | `CLIAgent` + plugins + screen_evidence | Expandir evidencia |
| Status blocked/working/done | `AgentTabStatus` + ops store + voice | Unificar con Agent Mode conversation state |
| Click focus | Workspace activate tab/pane | P0.3 |
| Spawn pane API | `WorkspaceAction` + future warp CLI | No socket clone |
| Detach server | Warp window/session model | No reimplementar herdr server |
| Multi-agent “team” feeling (video) | Super Agent multi + fleet rail | Producto objetivo del video |

---

## 8. Principios de implementación

1. **Inspiration only** de Herdr — zero copy de src/, manifests, skills wire.  
2. **PTY real primero** para CLIs; Agent Mode real para native seats.  
3. **Evidence-based status** — structured events &gt; scrap de scrollback.  
4. **Glance &gt; chrome** — si el rail no responde en &lt;5 s, falló el goal.  
5. **Un glass** — native + CLI en el mismo fleet; no dos apps mentales.  
6. **Switch de modelo** es first-class (Claude/Codex/Grok), no reabrir CLI.  
7. **Cap fan-out** — nunca spawnear 6+ tabs de golpe (freeze documentado).

---

## 9. Relación con trabajo ya hecho en el fork

| Pieza | Path / idea | Rol en este goal |
|-------|-------------|------------------|
| External agent monitor | `agent_tabs_projection`, ops | Base del fleet CLI |
| Subagent voice | `agent_subagent_voice` | Glance legible |
| Fleet / Local Agents | `fleet_room_modal`, Super Agent | Entry multi-connector |
| Native provider map | `native_agent_provider` | Claude/Codex/Grok → LLM |
| Session shell | `codex_shell` | Deep-dive subagents |
| Herdr-class draft | `herdr-class-agent-workspace.md` | Detalle UX anterior |

Este goal **absorbe y ordena** esos hilos bajo un único outcome: **Herdr-class nativo en Warp**.

---

## 10. Criterio de éxito (cierre del goal)

Dogfood de un día real:

1. Abrís Warp en un monorepo.  
2. Tenés **≥2 Agent Mode** (p.ej. Claude + Codex) y **≥1 CLI** (p.ej. Grok CLI o Codex CLI) vivos.  
3. El **fleet rail** muestra los tres con estado correcto.  
4. Uno pide permiso / se bloquea → lo ves en &lt;5 s y **un click** te lleva al pane.  
5. Switcheás modelo nativo Claude↔Codex↔Grok en Super Agent sin perder el hilo de control.  
6. No necesitás Herdr ni tmux para operar la flota.

Hasta que 1–6 pasen en dogfood, el goal **sigue abierto**.

---

## 11. Next (orden de ataque)

1. **P0.3** — focus confiable rail → pane/tab (native + CLI).  
2. **P1.1** — unificar Agent Mode seats en el mismo fleet que CLI.  
3. **P0.5** — dogfood 3-agent triage.  
4. **P1.3** — blocked/need-you con evidencia unificada.  
5. **P2** — layout persistence + alertas + vault.

---

## 12. Tracking

| Campo | Valor |
|-------|--------|
| Topic engram | `product/goal-herdr-native-warp` |
| Video | https://www.youtube.com/watch?v=Bu1kOdrXmoc |
| Repo insp. | https://github.com/ogulcancelik/herdr |
| Success gate | §10 dogfood 1–6 |
| Supersede/align | Refina `GOAL-multiagent-loop-terminal.md` hacia Herdr-class nativo |

---

## 13. Resumen brutal

| | Herdr | Warp-native goal |
|--|-------|------------------|
| Qué es | Multiplexor TUI de agents | **El mismo job**, embebido en Warp |
| Runtime | Solo CLI PTYs | CLI PTYs **+** Agent Mode nativo |
| Valor | Glance + focus + detach | Glance + focus + **switch multi-modelo** + un solo app |
| Cómo ganamos | No competir en TUI binario | Usar lo que Warp ya es: terminal + AI glass |

**No construimos un Herdr dentro de Warp.**  
**Hacemos que Warp sea el lugar donde el rebaño (Claude/Codex/Grok) se opera como Herdr — nativo.**
