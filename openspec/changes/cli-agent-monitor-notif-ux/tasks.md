## 1. Store / model

- [x] 1.1 `active_agent_count`, `remove_session`, `reconcile_for_terminal`, prune Reviewed on load
- [x] 1.2 Remove session after successful review (activate / mark)
- [x] 1.3 Dismiss `active_alert` without review; chip uses active count

## 2. Navigation

- [x] 2.1 Workspace activate: focus app, local + cross-window terminal, open panel
- [x] 2.2 Pet activate routes to workspace that owns the terminal

## 3. Pet UI

- [x] 3.1 Dismiss control on bubble
- [x] 3.2 Clear alert on successful activate

## 4. Verify

- [ ] 4.1 Unit tests for count, reconcile, remove-on-review
- [ ] 4.2 Manual: finish codex → bubble + OS notif → click → Warp focuses → chip drops
