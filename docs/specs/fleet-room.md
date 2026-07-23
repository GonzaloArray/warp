# Spec: Fleet Room (Local Agents group)

**Status:** implement MVP  
**Inspiration:** BLOOME-style multi-agent group chat (screenshot / product video)  
**Runtime truth:** each member is still a real CLI (Claude / Codex / Grok / …). Warp is the **room + router**, not a new model host.

## Problem

Users want one place to talk to **several local coding agents together** — list of agents, shared thread, `@` routing — instead of only N isolated terminal tabs.

## Goal (MVP)

A **Fleet Room** surface:

1. **Member list** — enabled hub providers as room members (Claude, Codex, Grok, …)
2. **Shared thread** — user + system + agent-routed messages
3. **Composer** — free text + `@Provider` / `@all`
4. **Route** — on send, launch (or focus) the targeted CLI(s); append route receipts to the thread
5. **Open from `+` menu** — “Local Agents · Sala multiagent”

## Non-goals (MVP)

- Full duplex transcript streaming from each CLI into the chat bubble (P1)
- Cross-agent tool calls inside one model context
- Collaboration-rules wizard (show stub card only)
- Obsidian persistence of the thread (next track)
- Replacing the vertical agent monitor

## Domain

```text
FleetRoom
  id, title ("Local Agents")
  members[]: FleetMember { provider, slot_label, status, ready }
  messages[]: FleetMessage { id, author, kind, body, created_ms }
  draft (composer text — UI may own EditorView)

FleetMessageKind: User | System | Agent | Route
FleetAuthor: User | System | Provider(AgentProviderId)
```

### Mention routing

| Token | Targets |
|-------|---------|
| `@all` / `@todos` / `@everyone` | all **ready** members (or all enabled if none ready) |
| `@claude` / `@codex` / … | that provider (aliases per `AgentProviderId`) |
| no mention | default: all ready members, else all enabled |

Only **enabled** hub providers can be members. Missing CLI on PATH → member `ready=false`, still listable with “instalar CLI”.

### Send pipeline

1. Validate non-empty draft  
2. Append `User` message  
3. Resolve targets  
4. For each target append `Route` (“Enviando a Codex…”)  
5. Return `FleetSendPlan { routes: [provider, prompt] }` for Workspace to execute  
6. Workspace: `LaunchAgentProvider` (NewSession) per route; later: inject prompt / focus existing session  

### Welcome thread

On first open of a room, seed one `Agent` intro line per member (BLOOME-style “Hi, I’m X…”), Spanish product copy.

## UI layout (MVP)

```
┌──────────────┬─────────────────────────────┐
│ Members (N)  │ Local Agents                │
│ Claude · …   │  [messages scroll]          │
│ Codex · …    │                             │
│ Grok · …     │  [composer]  [Enviar]       │
└──────────────┴─────────────────────────────┘
```

- Theme tokens only (no hard-coded brand RGB except provider accents already in `AgentUiProfile`)
- Close control returns to normal workspace
- Select member → optional focus filter (P1: filter thread; MVP: highlight only)

## Actions

| Action | Effect |
|--------|--------|
| `OpenFleetRoom` | open overlay, ensure members from hub prefs |
| `CloseFleetRoom` | hide overlay |
| `FleetRoomSend` | domain send + execute routes |
| `FleetRoomLaunchMember { provider }` | one-click launch that member’s CLI |
| `FleetRoomSelectMember { provider }` | highlight |

## Tests

Pure unit tests (no GPU):

1. mention parser: `@claude` / `@all` / none  
2. send appends user + route messages  
3. disabled providers never targeted  
4. welcome seeds one agent line per member  
5. empty draft is no-op  

## Success (dogfood)

Open Fleet Room → see Claude/Codex/Grok as members → type `@claude fix the test` → Enviar → Claude CLI tab launches and thread shows route receipt. Same for `@all`.
