# Spec: Subagent voice presentation (orchestrator view)

**Status:** implement  
**Branch context:** `feat/agent-task-hierarchy`  
**Product north star:** Warp as multi-agent terminal control plane — same *logic* as orchestrator UIs (lead + children), without becoming the inference runtime. Subagent copy must read like a coding agent speaking, not a raw event dump.

## Problem

Today child rows often show:

- bare status chips (`trabajando`)
- raw tool scraps (`web`, `herramienta: bash`, JSON fragments)
- path leaves / UUIDs without objective
- “anything that came from JSONL” without quality gates

That breaks the orchestrator mental model: when you glance a subagent you should understand **what it is doing for the lead**, the way Codex/Claude surface tasks in their own UIs.

## Goal

Every visible subagent line (rail + session shell + detail header) is produced by a **single pure voice layer** with explicit acceptance criteria. No UI path invents free-form strings from untrusted blobs.

## Non-goals

- Spawning or steering subagents inside Warp (CLI remains runtime)
- Cross-provider orchestration (Claude lead → Codex worker)
- Full transcript chat UI
- Obsidian persistence (separate track)

## Information model

Trusted sources only (already true for topology):

| Provider | Topology | Task / activity enrichment |
|----------|----------|----------------------------|
| Codex | `sub_agent_activity` JSONL | Child rollout: NEW_TASK, function_call, task_complete |
| Claude | `Agent`/`Task` tool_use + `subagents/*.jsonl` | description, subagent_type, status from file |
| Others | leaf (no children) | n/a until structured events exist |

Never: terminal stdout scraping for topology or voice.

## Voice schema

```text
SubagentVoice
  title          — human name (path leaf / description); never raw UUID alone
  speaking       — one sentence, agent voice (present progressive or done result)
  meta           — glance chips: status · elapsed · files (facts only)
  card_subtitle  — compact nav/rail: meta + speaking (or speaking if meta empty)
  now_line       — detail “Ahora · …” (optional)
  objective      — what the lead asked (optional, speakable only)
  outcome        — result when completed/failed (optional, speakable only)
```

### Agent voice (tone)

Speak **as the subagent reporting to the lead**, Spanish product copy:

| Status | Frame |
|--------|--------|
| Working | `Estoy …` |
| Waiting | `Espero tu input · …` |
| Blocked | `Necesito permiso · …` |
| Failed | `Fallé · …` |
| Completed | `Listo · …` |
| Unavailable | `Arrancando …` |

Prefer objective + current action over tool protocol names.

Good:

- `Estoy explorando el módulo de webhooks`
- `Estoy buscando en la web para auth OAuth`
- `Listo · tests del router en verde`
- `Necesito permiso · aplicar patch en app/src/auth.rs`

Bad (must not ship):

- `toolu_01abc…`
- `{"type":"function_call"…`
- `019f7d73-aaaa-7a02-…` as title
- `herramienta: bash · {"command":…`
- `No se realizaron cambios en archivos.` as speaking line
- Dump of system prompt / AGENTS.md

## Acceptance criteria (`is_speakable`)

A string may enter `speaking` / `objective` / `outcome` only if **all** pass:

1. Trimmed length in `[8, 160]` for speaking/objective/outcome (titles may be `[2, 48]`)
2. Not a UUID / thread id / tool id pattern
3. Not JSON-looking (`{` early + `}` or `":"` density)
4. Not mostly punctuation / base64
5. Not a known junk phrase list (empty work, system dump markers)
6. Not a raw shell dump longer than one clause without verb framing

If activity fails gates but task passes → speak from task.  
If both fail → status frame only: `Estoy trabajando` / `Listo` / etc. (never invent fake file names).

## UI wiring

| Surface | Field |
|---------|--------|
| Vertical rail child | `ops_primary` = `speaking` (truncated ~72); `ops_secondary` = `meta` |
| Shell nav card title | `title` (existing display_name if already good) |
| Shell nav card subtitle | `card_subtitle` |
| Detail header activity | `now_line` or `speaking` |
| Detail Objetivo / Resultado | `objective` / `outcome` when present |

Parent rollup (counts) stays factual: `N subagents · X trabajando · …` — not agent voice.

## Mapping rules (tools → verbs)

Activity normalizers (non-exhaustive):

| Signal | Verb phrase |
|--------|-------------|
| web / web_search / web__run | buscando en la web |
| read_file / cat / open | leyendo archivos |
| apply_patch / write / edit | editando el repo |
| shell / bash / cargo / npm / test | corriendo comandos |
| grep / search / rg | buscando en el código |

Compose: `Estoy {verb}` + optional ` para {objective_short}` when objective is speakable and not redundant.

## Tests (required)

Pure unit tests in `agent_subagent_voice_tests.rs`:

1. Rejects UUID-only and JSON scrap
2. Working + task + web activity → framed progressive Spanish
3. Completed + outcome → `Listo · …`
4. Blocked without task → `Necesito permiso`
5. card_subtitle prefers speakable content over status chip alone when content exists
6. Projection test: child `ops_primary` is voice speaking, not bare `trabajando` when activity/task present

## Success metric (dogfood)

Open a Codex session that spawns ≥2 subagents and a Claude session with Agent/Task: every child row answers in &lt;2s glance **what work is happening**, without opening the PTY. No raw tool protocol on the primary line.
