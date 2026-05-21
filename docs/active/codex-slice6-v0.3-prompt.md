# v0.3 Slice 6 — MCP Coordination Tools + Send-Back Routing

You are implementing Slice 6. Read `docs/BUILD-SPEC-v0.3.md` Slice 6 (lines 197-205), the 8-tool surface (lines 106-127), and decisions D8 / D10 / D11 / D12 / D13. Slice 5 wired the read tools and push channel; Slice 6 wires the coordination tools and the user→agent loop.

This is the load-bearing product slice. After this, the full agent workflow works end-to-end: agent calls `open_artifact` → AgentCanvas foregrounds and tracks the file → user reviews / comments / interacts → user clicks Send-back → agent receives `notifications/artifact_updated` with the user's note.

## What "done" looks like

1. **`open_artifact(path)` works end-to-end.** Agent calls it → app foregrounds via `app.show()` + `Window::set_focus()` + macOS dock activate; the file becomes the current focus in the content pane; if the path wasn't tracked yet, it's auto-tracked with `in_inbox = 1`; the just-arrived highlight pulses on the file row briefly.
2. **`attach_artifact(path)` marks a file as in-context for the calling MCP session.** New SQLite table `session_attachments`. If `also_pin: true` (default false), the file gets pinned too. Multiple sessions can attach the same file.
3. **`notify_user(severity, message, action?)` surfaces a toast in the AgentCanvas UI.** Existing toast component handles it; the toast can include a `{ label, artifact_path }` action that opens the artifact when clicked.
4. **`add_comment(path, anchor, body)` writes a comment to the sidecar from the agent's identity.** `Comment.author = "{persona}·{agent}"` per D11. UI sees it pop into the comments panel automatically (because the watcher fires on the sidecar JSON, OR we push a direct notification to the UI).
5. **Send-back routes through MCP.** When the user clicks Send-back:
   - Button label is **"Send back to {persona}·{agent}"** when one MCP session attached this file, **"Send back to..."** with a picker when multiple sessions attached, **"Send to Agent"** (existing clipboard fallback) when no MCP sessions attached.
   - On click: insert a row into `user_messages` for the target session(s); emit `notifications/artifact_updated { by: "user", note, action_verb }` to those sessions.
6. **Smoke test from CLI:**
   - Launch app + shim with `initialize` + `notifications/subscribe`.
   - Agent calls `attach_artifact("/path/to/file.md")` → `{ attached: true }`.
   - User clicks "Send back to cpo·claude" in the UI on that file (or invoke the existing Tauri send command directly for testing).
   - Shim stdout receives `notifications/artifact_updated { by: "user", note, action_verb }` within ~1s.

## Tool implementations (match spec lines 110-127)

### `open_artifact`

Args: `{ path: string }`.
Returns: `{ tracked: bool, was_already_known: bool }`.

Sequence:
1. Path safety check via `path_safe_for_canvas`.
2. Was the file already in `files`? Set `was_already_known` accordingly.
3. If unknown: insert row with `in_inbox = 1`, kick a watcher resync.
4. Call into a new Tauri-side helper that runs on the main thread:
   - `WebviewWindow::show()` + `set_focus()` (Tauri 2 API)
   - macOS: `[[NSApplication sharedApplication] activateIgnoringOtherApps: YES]` via `objc2` OR just emit a Tauri event that the frontend handles by `focus()` on `window` — pick the simpler approach. If you go the frontend route, emit `agentcanvas://focus-and-open { path }` and have App.tsx subscribe.
   - Set `current_focus` to the path.
5. Return `{ tracked: true, was_already_known }`.

The "just-arrived highlight" is an existing UI primitive (`arrival-dot` / `just-arrived` class) — wire it for paths opened via this command.

### `attach_artifact`

Args: `{ path: string, also_pin?: boolean = false }`.
Returns: `{ attached: bool }`.

New table:

```sql
CREATE TABLE IF NOT EXISTS session_attachments (
  session_id  TEXT NOT NULL,
  path        TEXT NOT NULL,
  attached_at INTEGER NOT NULL,
  PRIMARY KEY (session_id, path)
);
CREATE INDEX IF NOT EXISTS idx_session_attachments_path ON session_attachments(path);
```

Behavior:
1. Path safety check.
2. Upsert row (session_id from the calling MCP session, path, now).
3. If `also_pin`: also set `pinned=1` on the file row (auto-track if unknown).
4. Return `{ attached: true }`.

Cleanup: on connection close, delete all rows for that session_id.

### `notify_user`

Args: `{ severity: "info"|"warn"|"error", message: string, action?: { label, artifact_path } }`.
Returns: `{ delivered: bool }`.

Behavior:
1. Validate severity.
2. Emit Tauri event `agentcanvas://notify-user { severity, message, action }` on the main webview window.
3. Frontend subscribes; renders in the existing `handoffToast` slot (or extends it with severity-styled variants).
4. If `action` present, the toast renders an inline link button. Clicking calls `open_artifact(action.artifact_path)` in the same shape as MCP would.
5. Return `{ delivered: true }` if emit succeeded.

### `add_comment`

Args: `{ path, anchor, body }`.
Returns: `{ comment_id: string }`.

Behavior:
1. Path safety + tracked check.
2. Validate anchor against the discriminated union: `text_selection { block_id?, start_offset, end_offset }`, `html_selection { start_offset, end_offset, snapshot_text }`, or `file_level { kind: "file_level" }`.
3. Load existing sidecar (`sidecar::load_or_migrate` byte-safe path) → append new comment with:
   - `id`: new UUIDv4
   - `author`: `"{session.persona}·{session.agent}"` per D11
   - `created_at`: epoch seconds
   - `anchor`, `body`, `resolved: false`
4. Save sidecar.
5. Return `{ comment_id }`.
6. Optionally emit a `agentcanvas://comments-changed { path }` Tauri event so the UI refreshes if it's the open file.

### Session-scoped `list_artifacts` (now)

Slice 5 returned everything; Slice 6 applies session scoping. Default session view:
- Files with `in_inbox = 1` (Inbox)
- Files in the session's project, if the session declares one (already in clientInfo.agentCanvas.project)
- Files attached to this session via `session_attachments`
- Files with `pinned = 1` (always visible to all sessions)

If `filter` is provided, it overrides the default — agents can explicitly ask for archived files, etc. If no filter, apply the default mask.

## Send-back routing

### Backend wiring

The existing `send_files` Tauri command (or whatever produces the clipboard handoff today) needs to:
1. Check `session_attachments` for any sessions attached to this file.
2. If 0 sessions: fall through to existing clipboard path.
3. If 1+ sessions: for each, insert into `user_messages` (id, session_id, path, note, action_verb, created_at), then `mcp::emit_artifact_updated` to that session's notification channel with `by: "user", note, action_verb`.
4. Return a payload indicating routed-via-mcp vs clipboard so the UI can show the right confirmation.

### Frontend wiring

The Send-back button (find it in App.tsx — `sendFileFromMenu` or similar):
- Query backend for `session_attachments_for_path(path)` → list of `{ session_id, persona, agent, project }` sessions.
- Render label / behavior based on count:
  - 0 → existing "Send to Agent" + clipboard
  - 1 → "Send back to {persona}·{agent}" — calls a new `send_back_to_session(path, session_id, note, action_verb)` command
  - 2+ → dropdown picker, default = session with most recent `session_attachments.attached_at`

## Files you will touch / create

- `crates/agent-canvas-app/src/main.rs` — new Tauri commands (`send_back_to_session`, `session_attachments_for_path`), session_attachments migration, send-back routing wiring
- `crates/agent-canvas-app/src/mcp/tools.rs` — real implementations of open_artifact / attach_artifact / notify_user / add_comment + session-scoped list_artifacts default
- `crates/agent-canvas-app/src/mcp/sessions.rs` — attach/detach session helpers + cleanup on close
- `crates/agent-canvas-app/src/mcp/mod.rs` — connection-close attachment cleanup
- `ui/src/App.tsx` — Send-back button adaptation, picker, notify_user toast handling, open-via-mcp event handler
- `ui/src/ipc.ts` — new IPC wrappers (`sessionAttachmentsForPath`, `sendBackToSession`)
- `status.md`, `BACKLOG.md`

## Tests

Add to `crates/agent-canvas-app/src/mcp/mod.rs` `#[cfg(test)]`:

- `open_artifact_inserts_unknown_path_with_inbox_tag`
- `open_artifact_returns_was_already_known_for_tracked_path`
- `attach_artifact_inserts_session_attachment_row`
- `attach_artifact_with_also_pin_pins_file`
- `attach_artifact_cleanup_on_connection_close_removes_rows`
- `add_comment_appends_with_persona_agent_author`
- `add_comment_round_trips_through_sidecar`
- `notify_user_emits_tauri_event` (test the event payload shape)
- `list_artifacts_default_returns_inbox_plus_project_plus_attached_plus_pinned`
- `send_back_to_session_inserts_user_message_and_emits_notification`
- `session_attachments_migration_idempotent`

## Hard constraints

- A22 / A15 / A17 unchanged.
- Do not modify intent.md, BUILD-SPEC-v0.3.md, legacy/vellum-spec-v0.3.md.
- `Comment.author` MUST be `"{persona}·{agent}"` per D11 — single chip render.
- No agent write tools beyond `add_comment` (D14). Do not add `create_artifact`, `commit_edit`, `propose_edit`, etc.
- Resolved/done is implicit (D13). Do not add `mark_resolved` tool.
- Tool argument / return shapes must match spec lines 110-118 exactly — no invented fields.
- Path-safety check on every path-bearing tool. JSON-RPC error on rejection.
- Existing clipboard Send-back path must still work for files with no MCP session attached.
- Don't touch the watcher fix (276d52b).
- Don't touch sidecar / comment UI paths from Slice 1/2/3 except to wire `add_comment` author = persona·agent.

## Verification

```bash
cd crates/agent-canvas-app && cargo check -q
cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -25
cd crates/vellum-core && cargo test sidecar 2>&1 | tail -5
cd crates/agent-canvas-mcp && cargo build
cd ui && ./node_modules/.bin/tsc --noEmit
cd ui && ./node_modules/.bin/vite build 2>&1 | tail -3

# Invariants
grep -rn 'allow-same-origin\|allow-modals' ui/src/ | grep -v '\.test\.' | wc -l   # 0
```

All must pass.

## Report format

Write `docs/active/codex-slice6-v0.3-report-2026-05-21.md`:

1. Files modified / created
2. Per-tool: arg shape, return shape, body summary, error cases
3. session_attachments migration + cleanup-on-close path
4. Send-back routing decision tree (0 / 1 / 2+ sessions)
5. Tests added
6. Verification command outputs
7. Known issues / gaps

Commit:

```
feat(v0.3-slice6): open_artifact, notify_user, attach_artifact, add_comment, send-back push
```

Single atomic commit.

## Out of scope (do NOT build)

- Agent panel UI rows (Slice 7)
- Manual session add (Slice 7)
- Persona registry reload cross-MCP invalidation (Slice 7)
- One-click install for Claude Code / Codex / Cursor (Slice 7)
- CLAUDE.md template (Slice 7)
- Comment-thread replies
- Cross-session comment attribution beyond author = persona·agent
- Release work (Slice 8)

If you notice an improvement adjacent, write to `BACKLOG.md` with `[v0.3-slice6-spinoff]`. Do not build it.
