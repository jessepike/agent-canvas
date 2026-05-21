# v0.3 Slice 6 Report — MCP Coordination Tools + Send-Back Routing

Date: 2026-05-21
Implemented by: Codex (GPT Pro)
Planned by: Claude (Opus 4.7)

## 1. Files Modified / Created

- Modified `crates/agent-canvas-app/src/main.rs`
- Modified `crates/agent-canvas-app/src/mcp/tools.rs`
- Modified `crates/agent-canvas-app/src/mcp/sessions.rs`
- Modified `crates/agent-canvas-app/src/mcp/mod.rs`
- Modified `ui/src/App.tsx`
- Modified `ui/src/ipc.ts`
- Modified `status.md`
- Modified `BACKLOG.md`
- Created `docs/active/codex-slice6-v0.3-report-2026-05-21.md`

## 2. Per-Tool Implementation

### `open_artifact`

- Args: `{ path: string }`
- Returns: `{ tracked: bool, was_already_known: bool }`
- Body summary: validates the path with `path_safe_for_canvas`, requires a regular file, checks `files`, auto-tracks unknown files with `in_inbox = 1`, shows/focuses the main Tauri window, emits `agentcanvas://focus-and-open { path }`, sets `current_focus`, and refreshes watcher paths.
- Error cases: missing path, unsafe path, non-regular file, metadata/db failures.

### `attach_artifact`

- Args: `{ path: string, also_pin?: boolean }`
- Returns: `{ attached: bool }`
- Body summary: validates path, requires initialized MCP session, upserts `session_attachments(session_id, path, attached_at)`, auto-tracks unknown files, and sets `pinned = 1` when `also_pin` is true.
- Error cases: initialize missing, unsafe path, non-regular file, metadata/db failures.

### `notify_user`

- Args: `{ severity: "info"|"warn"|"error", message: string, action?: { label, artifact_path } }`
- Returns: `{ delivered: bool }`
- Body summary: validates severity/message/action shape, path-checks `action.artifact_path`, and emits `agentcanvas://notify-user` to the main webview. The UI renders it in the existing toast slot and opens the action artifact when clicked.
- Error cases: invalid severity, missing message, invalid action shape, unsafe action path, Tauri emit failure.

### `add_comment`

- Args: `{ path, anchor, body }`
- Returns: `{ comment_id: string }`
- Body summary: validates path and tracked state, validates `text_selection`, `html_selection`, or `file_level` anchor via the sidecar union, loads or creates the sidecar identity map, appends a UUIDv4 comment with `author = "{persona}·{agent}"`, saves the sidecar, and emits `agentcanvas://comments-changed { path }`.
- Error cases: initialize missing, unsafe path, non-regular file, untracked artifact, invalid anchor, missing body, sidecar read/write failure.

### `list_artifacts`

- Args: `{ filter?: { inbox?, project?, pinned?, archived? } }`
- Returns: `Vec<ArtifactSummary>`
- Body summary: explicit filters keep the existing tag-filter behavior. With no filter, the default session view is Inbox, the session project, session attachments, and pinned files.
- Error cases: db query errors and metadata hydration errors.

## 3. `session_attachments` Migration + Cleanup

Added idempotent migration:

```sql
CREATE TABLE IF NOT EXISTS session_attachments (
  session_id  TEXT NOT NULL,
  path        TEXT NOT NULL,
  attached_at INTEGER NOT NULL,
  PRIMARY KEY (session_id, path)
);
CREATE INDEX IF NOT EXISTS idx_session_attachments_path ON session_attachments(path);
```

`initialize_state_db` runs the migration. `mcp::sessions::cleanup_session_attachments` is called when an MCP connection closes, before the corresponding `agent_sessions.disconnected_at` update.

## 4. Send-Back Routing Decision Tree

- 0 attached MCP sessions: UI label remains `Send to Agent`; the existing clipboard handoff path is used.
- 1 attached MCP session: UI label becomes `Send back to {persona}·{agent}`; submit calls `send_back_to_session`.
- 2+ attached MCP sessions: UI label becomes `Send back to...`; the picker defaults to the most recently attached session.

`send_back_to_session` validates the file/session attachment, inserts a `user_messages` row, then emits `notifications/artifact_updated { path, by: "user", note, action_verb }` to the targeted session.

## 5. Tests Added

- `open_artifact_inserts_unknown_path_with_inbox_tag`
- `open_artifact_returns_was_already_known_for_tracked_path`
- `attach_artifact_inserts_session_attachment_row`
- `attach_artifact_with_also_pin_pins_file`
- `attach_artifact_cleanup_on_connection_close_removes_rows`
- `add_comment_appends_with_persona_agent_author`
- `add_comment_round_trips_through_sidecar`
- `notify_user_emits_tauri_event`
- `list_artifacts_default_returns_inbox_plus_project_plus_attached_plus_pinned`
- `send_back_to_session_inserts_user_message_and_emits_notification`
- `session_attachments_migration_idempotent`

## 6. Verification Command Outputs

```text
cd crates/agent-canvas-app && cargo check -q
passes with pre-existing ts-rs sidecar warning
```

```text
cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -25
test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.45s
```

```text
cd crates/vellum-core && cargo test sidecar 2>&1 | tail -5
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out; finished in 0.00s
```

```text
cd crates/agent-canvas-mcp && cargo build
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.05s
```

```text
cd ui && ./node_modules/.bin/tsc --noEmit
passes
```

```text
cd ui && ./node_modules/.bin/vite build 2>&1 | tail -3
- Use build.rollupOptions.output.manualChunks to improve chunking: https://rollupjs.org/configuration-options/#output-manualchunks
- Adjust chunk size limit for this warning via build.chunkSizeWarningLimit.
✓ built in 983ms
```

```text
grep -rn 'allow-same-origin\|allow-modals' ui/src/ | grep -v '\.test\.' | wc -l
0
```

## 7. Known Issues / Gaps

- The requested CLI smoke path was covered by unit-level MCP/session tests in this pass, not by launching the full GUI and shim together. The app/socket smoke should be run once the desktop app is already running in the user's session.
- `cargo check` still prints the pre-existing `ts-rs` warning for `CommentAnchor` serde attributes. This is already tracked as a v0.3 spinoff.
- Vite still reports the known large-chunk warning.
