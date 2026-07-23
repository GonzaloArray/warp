# Goal: Warp como terminal multiagent + loop engineering

**Status:** active goal  
**Branch:** `feat/agent-task-hierarchy`  
**Fecha:** 2026-07-23  
**Owner product:** Gonzalo (fork)  

**Referencias de producto (inspiración, no clon):**

| Fuente | Qué tomamos |
|--------|-------------|
| [Warp · coding agents](https://www.youtube.com/watch?v=Fw4wBzmSX_8) | One-click al CLI por modelo |
| [Warp · multi-agent orchestration](https://www.youtube.com/watch?v=RWT3sh68PWE) | Lead + subagents visibles |
| BLOOME Local Agents (screenshot) | Grupo multiagent: un hilo, N cerebros, `@` |
| Herdr (AGPL) | Glanceable fleet + focus al PTY real — **solo modelo mental** |
| Obsidian | System of record del loop (markdown nativo) |

---

## 0. Una frase

Que Warp sea el **OS multiagent del loop engineering**: lanzás Claude / Codex / Grok / … en un click, los **ves y controlás** como flota (y como subagents del lead), les hablás desde una **sala grupal**, y **toda la evidencia del loop** queda en un vault Obsidian nativo — sin que Warp sea el runtime de inferencia.

---

## 1. Problema

Hoy el coding multiagent es fricción:

1. Abrir N terminales, recordar comandos, PATH, resume.  
2. No hay un lugar que diga *quién trabaja / quién te necesita / qué hizo el subagent*.  
3. No hay un hilo de grupo (tipo BLOOME) sobre CLIs reales.  
4. El aprendizaje del loop se pierde en chats y PTY scrollback.

Queremos **cerrar el loop** en Warp:

```
goal → launch fleet → observar/controlar → capturar evidencia → próximo loop
         ↑______________________________________________________|
                         vault Obsidian
```

---

## 2. Principios (no negociables)

1. **CLI = runtime.** Claude / Codex / Grok son los agentes. Warp observa, rutea y presenta.  
2. **PTY real.** Click / Abrir CLI = focus o spawn de terminal real, no un fake chat.  
3. **Topología confiable.** Solo eventos estructurados (JSONL / tool_use). Nunca scrap de stdout para topology.  
4. **Voz con criterio.** El copy de subagents pasa gates (`is_speakable`); habla como agente, no como log.  
5. **Memoria en disco legible.** Obsidian/markdown es system of record; no solo SQLite opaco.  
6. **No clonar productos con licencia conflictiva.** Herdr/BLOOME = inspiración de UX, no código.

---

## 3. Tres capas del producto

| Capa | Nombre | Rol |
|------|--------|-----|
| **Runtime** | Tabs CLI | Procesos `claude` / `codex` / `grok` / … |
| **Control** | Monitor + voice + Fleet Room | Glance, deep-dive, grupo `@` |
| **Memory** | Vault Obsidian | Goals, runs, evidencia, decisiones |

```
┌─ Fleet Room (grupo) ─────────────────────────┐
│  @claude @codex @all · hilo compartido       │
└──────────────────┬───────────────────────────┘
                   │ rutea
     ┌─────────────┼─────────────┐
     ▼             ▼             ▼
  tab Claude    tab Codex     tab Grok     ← Runtime
     │             │             │
     └────── Monitor + voice ────┘         ← Control (subagents del lead)
                   │
                   ▼
            vault Obsidian                 ← Memory
```

---

## 4. Outcomes medibles (Definition of Done del goal)

### P0 — Dogfood diario (parcialmente hecho)

| # | Outcome | Estado |
|---|---------|--------|
| P0.1 | One-click por provider al CLI desde `+` | ✅ |
| P0.2 | Monitor rail: sesiones CLI + subagents Codex/Claude | ✅ |
| P0.3 | Voice de subagents (Estoy/Listo/Necesito…) con gates | ✅ |
| P0.4 | Fleet Room: members + thread + `@` + Enviar → tabs | ✅ MVP |
| P0.5 | Dogfood: 1 sesión real Claude + 1 Codex con subagents legibles en rail | ⬜ |

### P1 — Sala que se siente BLOOME

| # | Outcome | Estado |
|---|---------|--------|
| P1.1 | Respuestas/estado del CLI vuelven al hilo de la Fleet Room (no solo route receipt) | ⬜ |
| P1.2 | Focus member → terminal de ese provider | ⬜ |
| P1.3 | Re-usar sesión viva en vez de siempre tab nuevo (cuando hay session_id) | ⬜ |
| P1.4 | Collaboration rules stub → prefs reales (roles / cuándo pinguear) | ⬜ |

### P2 — Loop engineering + Obsidian

| # | Outcome | Estado |
|---|---------|--------|
| P2.1 | Vault linkeado al workspace (path configurable) | ⬜ |
| P2.2 | Al start/done/need-you de un agent: nota markdown en vault | ⬜ |
| P2.3 | Fleet Room puede adjuntar `[[links]]` del vault al prompt ruteado | ⬜ |
| P2.4 | Historial de runs del loop legible en Obsidian sin abrir Warp | ⬜ |

### P3 — Polish de flota

| # | Outcome | Estado |
|---|---------|--------|
| P3.1 | Toasts de attention multi-provider consistentes | ⬜ parcial |
| P3.2 | Grok/Gemini topology cuando el CLI exponga eventos | ⬜ |
| P3.3 | E2E screenshot/guide de Fleet Room en dogfood | ⬜ |

---

## 5. Qué ya existe (baseline 2026-07-23)

| Pieza | Path / commit ref |
|-------|-------------------|
| Provider hub + launch | `agent_provider_hub.rs`, `LaunchAgentProvider` |
| Subagent topology | `agent_tabs_projection` (Codex JSONL, Claude tool_use) |
| Subagent voice | `agent_subagent_voice.rs` + spec |
| Session shell | `codex_shell.rs` / `codex_session_shell` |
| Fleet Room MVP | `agent_fleet_room.rs` + UI + `OpenFleetRoom` |
| Specs | `fleet-room.md`, `subagent-voice-presentation.md`, `herdr-class-agent-workspace.md` |

Branch: `feat/agent-task-hierarchy` (fork).

---

## 6. Anti-goals

- Reimplementar Claude/Codex como agentes nativos de Warp.  
- Chat SaaS desconectado del terminal.  
- “Todo vale” en copy de subagents (sin gates).  
- Clonar Herdr/BLOOME pixel a pixel o copiar su código.  
- Feature flag soup sin dogfood.

---

## 7. Criterio de éxito del goal (cierre)

El goal se considera **logrado** cuando, en un día de trabajo real:

1. Abrís Fleet Room, mandás `@claude` + `@codex` un objetivo de loop.  
2. Ves ambos en el monitor; subagents de cada uno se leen en voz de agente.  
3. Saltás al PTY correcto en &lt;5 s cuando algo te necesita.  
4. Al terminar, el vault Obsidian tiene el run (objetivo, quién, outcome) sin export manual.

Hasta que 1–4 pasen en dogfood, el goal **sigue abierto**.

---

## 8. Next (orden de ataque)

1. **P0.5** dogfood Fleet Room + rail con Claude y Codex reales.  
2. **P1.1–P1.2** estado/respuesta en hilo + focus a terminal.  
3. **P2.1–P2.2** vault Obsidian write path.  
4. Cerrar P1/P2; P3 solo si fricción real.

---

## 9. Tracking

| Campo | Valor |
|-------|--------|
| Topic engram | `product/goal-multiagent-loop` |
| Specs hijas | `fleet-room.md`, `subagent-voice-presentation.md`, `herdr-class-agent-workspace.md` |
| Success gate | §7 dogfood 1–4 |
