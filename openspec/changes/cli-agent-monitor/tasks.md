## 1. Domain model

- [x] 1.1 Create `app/src/terminal/cli_agent_monitor/` module (state, session, priority, store)
- [x] 1.2 Unit tests: finish → COMPLETED_UNSEEN + pending; reload persistence; dismiss/hover/timeout do not REVIEWED; navigation success → REVIEWED; missing terminal → explicit mark only

## 2. Detection adapters

- [x] 2.1 Common `AgentAdapter` + registry; Claude, Codex, Grok adapters
- [x] 2.2 Unit tests with process/command fixtures + multi-instance + fake extra agent
- [x] 2.3 Add `CLIAgent::Grok` and command-prefix detection in `cli_agent.rs`

## 3. Live integration

- [x] 3.1 Monitor model singleton: ingest `CLIAgentSessionsModel` status/events; emit notification requests on first COMPLETED_UNSEEN
- [x] 3.2 Navigation helper: focus existing pane → NavigationResult; apply review only on success

## 4. UI

- [x] 4.1 Header toolbar item + compact indicator string builder (`N agentes · M para revisar`)
- [x] 4.2 Panel rows: agent, project, state, elapsed, review-pending + semantic colors
- [x] 4.3 Wire click: navigate or “Marcar como revisado”

## 5. Verification

- [x] 5.1 Run package/unit tests; capture logs under scratch
- [x] 5.2 OpenSpec validate/verify; static UI surface evidence
