# AgentCanvas v0.3 Slice 5 Report — MCP Read Tools + Push Channel

Date: 2026-05-21
Implemented by: Codex

## 1. Files modified / created

Modified:
- `Cargo.toml`
- `Cargo.lock`
- `crates/agent-canvas-app/Cargo.toml`
- `crates/agent-canvas-app/src/main.rs`
- `crates/agent-canvas-app/src/mcp/mod.rs`
- `crates/agent-canvas-app/src/mcp/sessions.rs`
- `crates/agent-canvas-app/src/mcp/tools.rs`
- `ui/src/App.tsx`
- `ui/src/ipc.ts`
- `status.md`
- `BACKLOG.md`

Created:
- `crates/agent-canvas-app/src/mcp/notifications.rs`
- `docs/active/codex-slice5-v0.3-report-2026-05-21.md`

## 2. Per-tool returned shape

Tool results use MCP-style `content` plus `structuredContent`. The `structuredContent` value is the spec payload.

`list_artifacts`:

```json
{
  "content": [{ "type": "text", "text": "[{\"path\":\"/abs/note.md\",\"name\":\"note.md\",\"kind\":\"md\",\"mtime\":1,\"persona\":\"claude\",\"comment_count\":0}]" }],
  "structuredContent": [
    {
      "path": "/abs/note.md",
      "name": "note.md",
      "kind": "md",
      "mtime": 1,
      "persona": "claude",
      "comment_count": 0
    }
  ]
}
```

`get_artifact` for Markdown:

```json
{
  "content": [{ "type": "text", "text": "{\"base_hash\":\"<blake3-hex>\",\"kind\":\"md\",\"sidecar\":null,\"source\":\"# Hi\\n\"}" }],
  "structuredContent": {
    "source": "# Hi\n",
    "base_hash": "<blake3-hex>",
    "sidecar": null,
    "kind": "md"
  }
}
```

`get_artifact` for PNG:

```json
{
  "structuredContent": {
    "source": "iVBORw0KGgo=",
    "source_encoding": "base64",
    "base_hash": "<blake3-hex>",
    "sidecar": null,
    "kind": "png"
  }
}
```

`source_encoding: "base64"` is the only extra field beyond the spec shape; it is required so binary `source` can be decoded unambiguously.

`get_current_focus`:

```json
{
  "structuredContent": { "path": "/abs/path.md" }
}
```

No focus:

```json
{
  "structuredContent": null
}
```

`get_comments`:

```json
{
  "structuredContent": [
    {
      "id": "new",
      "author": "codex",
      "created_at": 20,
      "anchor": { "kind": "file_level" },
      "body": "new",
      "resolved": false
    }
  ]
}
```

`get_user_messages`:

```json
{
  "structuredContent": [
    {
      "id": "m1",
      "session_id": "s1",
      "path": "/x.md",
      "note": "note",
      "action_verb": "Review",
      "created_at": 10
    }
  ]
}
```

## 3. Subscription state model

```rust
#[derive(Clone)]
pub struct Subscription {
    pub artifact_updated: bool,
    pub artifact_focused: bool,
    pub tx: mpsc::UnboundedSender<JsonRpcNotification>,
}

#[derive(Clone, Default)]
pub struct SubscriptionRegistry {
    inner: Arc<parking_lot::Mutex<HashMap<String, Subscription>>>,
}
```

Lifecycle:
- A connection gets a notification channel at accept time.
- After successful `initialize`, the server inserts the `agent_sessions` row and registers the session with `artifact_updated = true`, `artifact_focused = false`.
- `notifications/subscribe` replaces the two known mask flags from `params.events`; unknown event names are ignored.
- On EOF/error, the session is removed from the registry and its `agent_sessions.disconnected_at` is updated.
- Dropping the registered tx stops delivery to closed writer tasks.

## 4. Event-dispatch sketch

```rust
pub fn dispatch_artifact_updated(
    subscriptions: &SubscriptionRegistry,
    path: String,
    by: &str,
    note: Option<String>,
    action_verb: Option<String>,
) -> usize {
    let notification = JsonRpcNotification::artifact_updated(path, by, note, action_verb);
    subscriptions.dispatch_artifact_updated(notification)
}
```

The registry snapshots target senders while holding the parking_lot mutex, drops the lock, sends notifications, then removes stale sessions in a second short lock.

Event sources wired in Slice 5:
- watcher callback for tracked paths emits `notifications/artifact_updated { path, by: "watcher" }`
- successful `write_document` emits the same update immediately so app saves do not depend on watcher timing
- `set_current_focus(path)` stores focus and emits `notifications/artifact_focused { path }`
- `emit_artifact_updated(path, by, note, action_verb)` Tauri command exists for manual Slice 5 testing and Slice 6 reuse

## 5. `user_messages` migration SQL + idempotency proof

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

Idempotency proof: `mcp::tests::user_messages_migration_idempotent` runs the migration twice against an in-memory DB, inserts one row, and verifies `COUNT(*) = 1`.

## 6. Tests added

All pass:
- `subscribe_updates_session_mask`
- `default_subscription_includes_artifact_updated`
- `default_subscription_excludes_artifact_focused`
- `event_dispatch_filters_by_subscription_mask`
- `get_artifact_returns_base64_for_png`
- `get_artifact_returns_string_for_markdown`
- `get_comments_respects_since_filter`
- `get_user_messages_filters_by_session_id`
- `set_current_focus_then_get_current_focus_round_trips`
- `user_messages_migration_idempotent`

Related host test cleanup also landed:
- macOS tempdir path tests now allocate inside the repo working directory instead of denied `/var/folders`
- legacy tag backfill now compares DB path strings against the supplied legacy root without canonicalizing only one side

## 7. Verification command outputs

`cd crates/agent-canvas-app && cargo check -q`

```text
warning: failed to parse serde attribute
  |
  | #[serde(skip_serializing_if = "Option::is_none")]
  |
  = note: ts-rs failed to parse this attribute. It will be ignored.
```

`cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -25`

```text
test mcp::tests::event_dispatch_filters_by_subscription_mask ... ok
test tests::file_level_comment_anchor_round_trips ... ok
test tests::legacy_comment_anchor_deserializes_as_text_selection ... ok
test mcp::tests::tools_list_returns_nine_tools_with_input_schemas ... ok
test tests::send_payload_omits_empty_note_and_defaults_action ... ok
test tests::send_payload_uses_relative_path_fence_note_and_action ... ok
test tests::test_path_safe_for_canvas_allow_deny_matrix ... ok
test tests::test_path_within_canvas_shim_accepts_safe_path ... ok
test tests::test_path_within_canvas_resolves_symlinks ... ok
test mcp::tests::user_messages_migration_idempotent ... ok
test mcp::tests::agent_sessions_migration_idempotent ... ok
test tests::test_identity_relink_skips_when_old_path_exists ... ok
test mcp::tests::tools_call_stub_returns_method_not_found_for_unimplemented ... ok
test mcp::tests::set_current_focus_then_get_current_focus_round_trips ... ok
test mcp::tests::get_artifact_returns_string_for_markdown ... ok
test mcp::tests::get_user_messages_filters_by_session_id ... ok
test tests::untrack_keeps_file_delete_from_disk_removes_file ... ok
test tests::migration_backfills_legacy_tags_idempotently ... ok
test mcp::tests::get_artifact_returns_base64_for_png ... ok
test mcp::tests::get_comments_respects_since_filter ... ok
test mcp::tests::initialize_with_valid_clientinfo_returns_serverinfo ... ok
test mcp::tests::initialize_with_unknown_persona_accepts_with_warning ... ok

test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```

`cd crates/agent-canvas-mcp && cargo build`

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.89s
```

`cd ui && ./node_modules/.bin/tsc --noEmit`

```text
passes with no output
```

`cd ui && ./node_modules/.bin/vite build 2>&1 | tail -3`

```text
- Use build.rollupOptions.output.manualChunks to improve chunking: https://rollupjs.org/configuration-options/#output-manualchunks
- Adjust chunk size limit for this warning via build.chunkSizeWarningLimit.
✓ built in 1.09s
```

Invariant audit:

```text
$ grep -rn 'allow-same-origin\|allow-modals' ui/src/ | grep -v '\.test\.' | wc -l
0
```

CLI initialize smoke:

```text
{"id":1,"jsonrpc":"2.0","result":{"capabilities":{"tools":{}},"protocolVersion":"2025-06-18","serverInfo":{"name":"AgentCanvas","version":"0.3.0"}}}
```

Full watcher-to-shim stdout smoke was not completed in this shell because the available socket belonged to an already-running app instance. Rerun it with the freshly built GUI app launched so the socket server definitely includes Slice 5.

## 8. Known issues / gaps for Slice 6

- Send-back writes to `user_messages` are intentionally not wired yet.
- Session/file attachment targeting is still deferred, so Slice 5 broadcasts by subscription mask only.
- `open_artifact`, `notify_user`, `attach_artifact`, and `add_comment` remain Slice 6 tools.
- The pre-existing ts-rs warning for `CommentAnchor` remains tracked as a v0.3 spinoff.
