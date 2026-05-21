# v0.3 Slice 5 — MCP Read Tools + Push Notification Channel

You are implementing Slice 5 of AgentCanvas v0.3. Read `docs/BUILD-SPEC-v0.3.md`, especially Slice 5 (lines 187-195), the 8-tool surface (lines 106-127), notifications table (lines 120-126), and decisions D10 / D12.

Slice 4 built the protocol skeleton (`crates/agent-canvas-app/src/mcp/`). Slice 5 makes the read tools real and adds the server→client push channel.

## What "done" looks like

1. **Real `get_artifact`, `get_current_focus`, `get_comments`, `get_user_messages` tools.** Connect to the same code paths the UI uses. `list_artifacts` already proof-of-life — keep it working, just align its shape to the rest.
2. **`notifications/artifact_updated` pushed automatically** when:
   - The user-side watcher fires for a tracked file (file modified outside the app, OR by the app's own save).
   - The user clicks Send-back (Slice 6 will populate; Slice 5 just emits if the event is triggered manually for now — fall-through is fine).
3. **`notifications/artifact_focused` pushed** when the user selects a file in the UI, gated by per-session subscription.
4. **`notifications/subscribe`** method works: client sends `{ events: ["artifact_updated", "artifact_focused"] }`, server stores the subscription mask, future events filter by it. Default after `initialize`: `artifact_updated` subscribed, `artifact_focused` opt-in.
5. **End-to-end smoke from CLI:** launch shim, send `initialize`, edit a tracked file with `echo "foo" >> file.md` → a JSON-RPC notification appears on the shim's stdout.

## Tool implementations (do NOT invent new shapes — match BUILD-SPEC-v0.3 lines 110-118)

### `list_artifacts`

Args: `{ filter?: { inbox?: bool, project?: string, pinned?: bool, archived?: bool } }`.
Returns: `Vec<ArtifactSummary>` where `ArtifactSummary = { path, name, kind, persona, mtime, comment_count }`.
- `kind`: derived from extension (md / html / json / txt / png / pdf / other)
- Apply filters by AND. `inbox: true` means `files.in_inbox = 1`. `project: "foo"` means `files.project_tag = "foo"`. `pinned: true` means pinned. `archived: true` means archived. Missing filter = no constraint. **Session scoping is deferred to Slice 6** — for Slice 5, return everything matching the filter regardless of session.

### `get_artifact`

Args: `{ path }`.
Returns: `{ source, base_hash, sidecar, kind }`.
- `source`: file bytes. Encode as UTF-8 string for text kinds (md/html/json/txt). For binary kinds (png/pdf), return base64-encoded bytes with a `source_encoding: "base64"` field added to the result.
- `base_hash`: blake3 of bytes (hex string).
- `sidecar`: the `IdentityMap` (comments + base_snapshot), serialized as-is. `null` if no sidecar exists.
- `kind`: same as `list_artifacts`.
- Path safety: must pass `path_safe_for_canvas`. Otherwise return JSON-RPC error.

### `get_current_focus`

Args: `{}`.
Returns: `{ path }` or `null`.
- New field on `AppState`: `current_focus: Arc<Mutex<Option<String>>>` (or similar — pick whatever is consistent with the existing AppState shape).
- New Tauri command `set_current_focus(path: String)` called from the UI whenever the user selects a file. Path is the absolute path the user is viewing.
- Tool returns whatever's in the mutex.

### `get_comments`

Args: `{ path, since?: integer }`.
Returns: `Vec<Comment>` from the sidecar.
- Reuse the byte-safe `sidecar::load_or_migrate` work (binary files OK now).
- If `since` given, filter `c.created_at >= since`.
- No sidecar → empty array.

### `get_user_messages`

Args: `{ since?: integer }`.
Returns: `Vec<UserMessage>` where `UserMessage = { id, session_id, path, note?, action_verb?, created_at }`.
- **New SQLite table `user_messages`** (idempotent migration). Schema:
  ```sql
  CREATE TABLE IF NOT EXISTS user_messages (
    id           TEXT PRIMARY KEY,
    session_id   TEXT NOT NULL,
    path         TEXT NOT NULL,
    note         TEXT,
    action_verb  TEXT,
    created_at   INTEGER NOT NULL
  );
  CREATE INDEX IF NOT EXISTS idx_user_messages_session ON user_messages(session_id);
  CREATE INDEX IF NOT EXISTS idx_user_messages_created ON user_messages(created_at);
  ```
- For Slice 5, just expose the read. Writes come in Slice 6 (Send-back wires here).
- Filter by `session_id` of the calling session AND optional `since`.

## Push notification channel

This is the load-bearing part of Slice 5. The architecture:

### Per-session subscription state

Add to the session struct (or a parallel `Arc<Mutex<HashMap<session_id, Subscription>>>`):

```rust
struct Subscription {
    artifact_updated: bool,       // default true on initialize
    artifact_focused: bool,       // default false; opt-in
    /// Channel to push notifications down to the connection writer task.
    tx: tokio::sync::mpsc::UnboundedSender<JsonRpcNotification>,
}
```

When a connection accepts and finishes `initialize`:
1. Create the mpsc channel (`UnboundedSender` + `UnboundedReceiver`).
2. Store the tx end in the subscription map keyed by `session_id`.
3. Spawn a writer task that reads from the rx end and writes lines to the socket.
4. The existing reader task handles requests as today.

### `notifications/subscribe` method

```json
{ "method": "notifications/subscribe", "params": { "events": ["artifact_updated", "artifact_focused"] } }
```

Updates the session's flags. Returns `{}` success. Unknown event names: silently ignored (forward-compat).

### Event sources (Slice 5 wires three)

1. **File watcher fires** → enumerate all live sessions with `artifact_updated == true` → push `{ method: "notifications/artifact_updated", params: { path, by: "watcher" } }` down each tx.
2. **UI selects an artifact** → call `set_current_focus(path)` from the frontend → emit `{ method: "notifications/artifact_focused", params: { path } }` to all sessions with `artifact_focused == true`.
3. **`emit_artifact_updated(path, by, note?, action_verb?)`** helper exists for Slice 6 to call when Send-back fires. For Slice 5, do not wire the UI side — just make the function exist and is reachable from a Tauri command for testing.

### Connection cleanup

When a connection closes (EOF / error): remove its `session_id` from the subscription map; update the `agent_sessions` row's `disconnected_at`. Dropping the tx end auto-stops the writer task.

## Files you will touch / create

- `crates/agent-canvas-app/src/mcp/mod.rs` — wire writer task, subscription map, event dispatch
- `crates/agent-canvas-app/src/mcp/tools.rs` — real implementations for the 4 read tools
- `crates/agent-canvas-app/src/mcp/sessions.rs` — extend with subscription state, registration / removal helpers
- `crates/agent-canvas-app/src/mcp/notifications.rs` — new: notification types, dispatch helper, subscribe handler
- `crates/agent-canvas-app/src/main.rs` — `set_current_focus` Tauri command; hook the existing watcher path; AppState fields; user_messages migration
- `ui/src/App.tsx` — call `setCurrentFocus(path)` whenever `selectedPath` changes
- `ui/src/ipc.ts` — `setCurrentFocus` wrapper
- `status.md`, `BACKLOG.md`
- New tests (see below)

## Tests

Add to `crates/agent-canvas-app/src/mcp/mod.rs` `#[cfg(test)]`:

- `subscribe_updates_session_mask`
- `default_subscription_includes_artifact_updated`
- `default_subscription_excludes_artifact_focused`
- `event_dispatch_filters_by_subscription_mask`
- `get_artifact_returns_base64_for_png` (use a tiny in-test PNG byte stream)
- `get_artifact_returns_string_for_markdown`
- `get_comments_respects_since_filter`
- `get_user_messages_filters_by_session_id`
- `set_current_focus_then_get_current_focus_round_trips`
- `user_messages_migration_idempotent`

## Hard constraints

- A22 / A15 / A17 unchanged.
- Do not modify Slice 1/2/3 comment / sidecar / viewer code paths.
- Do not implement Send-back UI changes (Slice 6).
- Do not implement agent panel UI (Slice 7).
- Do not implement one-click install (Slice 7).
- Tool argument / return shapes must match `BUILD-SPEC-v0.3.md` lines 110-118. No invented fields except `source_encoding` for binary `get_artifact` (justify in report).
- Path-safety check on every path-bearing tool. JSON-RPC error on rejection.
- Session map mutations must be lock-free or use `parking_lot::Mutex` if you adopt it — block-and-hold over async boundaries is a bug.

## Verification you must run

```bash
cd crates/agent-canvas-app && cargo check -q
cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -25
cd crates/agent-canvas-mcp && cargo build
cd ui && ./node_modules/.bin/tsc --noEmit
cd ui && ./node_modules/.bin/vite build 2>&1 | tail -3

# Invariant audits
grep -rn 'allow-same-origin\|allow-modals' ui/src/ | grep -v '\.test\.' | wc -l   # must be 0
```

All must pass. Failed tests block commit.

## Report format

Write `docs/active/codex-slice5-v0.3-report-2026-05-21.md`:

1. Files modified / created
2. Per-tool returned-shape (paste the actual JSON-RPC response for each, recorded from a unit test)
3. Subscription state model (the struct + map + lifecycle)
4. Event-dispatch sketch (paste the dispatch function)
5. `user_messages` migration SQL + idempotency proof
6. Tests added (list, with pass/fail)
7. Verification command outputs
8. Known issues / gaps for Slice 6

Commit message:

```
feat(v0.3-slice5): MCP read tools + push notification channel
```

Single atomic commit.

## Out of scope (do NOT build)

- Send-back UI button (Slice 6)
- Send-back wiring `notifications/artifact_updated by: "user"` (Slice 6)
- Multi-session targeting picker (Slice 6)
- Agent panel rows (Slice 7)
- One-click install (Slice 7)
- CLAUDE.md template (Slice 7)
- Session-scoped `list_artifacts` filtering (Slice 6 — defaults will become Inbox + assigned project + Pinned)

If you notice an improvement adjacent, write to `BACKLOG.md` with `[v0.3-slice5-spinoff]`. Do not build it.
