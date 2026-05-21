# Codex Slice 4 v0.3 Report — MCP Server Skeleton

## 1. Files Modified / Created

- Modified `Cargo.toml`, `Cargo.lock`
- Modified `crates/agent-canvas-app/Cargo.toml`
- Modified `crates/agent-canvas-app/src/main.rs`
- Created `crates/agent-canvas-app/src/mcp/mod.rs`
- Created `crates/agent-canvas-app/src/mcp/sessions.rs`
- Created `crates/agent-canvas-app/src/mcp/tools.rs`
- Created `crates/agent-canvas-mcp/Cargo.toml`
- Created `crates/agent-canvas-mcp/src/main.rs`
- Modified `status.md`, `BACKLOG.md`

## 2. Socket Bind / Unlink Behavior

Actual init code:

```rust
pub fn init_mcp_server(app_handle: AppHandle) -> Result<(), String> {
    let socket_path = mcp_socket_path()?;
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let _ = fs::remove_file(&socket_path);

    let (shutdown, shutdown_rx) = watch::channel(false);
    let control = Arc::new(McpControl {
        socket_path: socket_path.clone(),
        shutdown,
        clients: Mutex::new(Vec::new()),
    });
    let _ = MCP_CONTROL.set(Arc::clone(&control));

    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_listener(app_handle, control, shutdown_rx).await {
            eprintln!("AgentCanvas MCP server stopped: {error}");
        }
    });

    Ok(())
}
```

The fixed socket path is `~/Library/Application Support/AgentCanvas/mcp.sock`; the parent directory is created and any stale socket file is removed before `UnixListener::bind`.

## 3. Stdio Shim Auto-Launch Flow

Actual auto-launch block:

```rust
async fn connect_with_launch(path: &PathBuf) -> Result<UnixStream, String> {
    if let Ok(stream) = UnixStream::connect(path).await {
        return Ok(stream);
    }

    Command::new("open")
        .args(["-a", "AgentCanvas.app"])
        .spawn()
        .map_err(|error| format!("failed to launch AgentCanvas.app: {error}"))?;

    for _ in 0..25 {
        sleep(Duration::from_millis(200)).await;
        if let Ok(stream) = UnixStream::connect(path).await {
            return Ok(stream);
        }
    }

    Err(format!(
        "AgentCanvas did not create MCP socket within 5s: {}",
        path.display()
    ))
}
```

On failure, the shim writes a JSON-RPC error response to stdout and exits nonzero.

## 4. `agent_sessions` Migration SQL + Idempotency

Required MCP table SQL:

```sql
CREATE TABLE IF NOT EXISTS agent_sessions (
  session_id      TEXT NOT NULL,
  source          TEXT NOT NULL,
  persona         TEXT NOT NULL,
  agent           TEXT NOT NULL,
  project         TEXT NOT NULL,
  connected_at    INTEGER NOT NULL,
  disconnected_at INTEGER,
  PRIMARY KEY (session_id, connected_at)
);
```

The old manual panel table previously used the name `agent_sessions`. Migration detects the old `backbone` column, renames that table to `manual_agent_sessions`, creates `manual_agent_sessions` if absent, then creates the new MCP `agent_sessions` table. The unit test `agent_sessions_migration_idempotent` runs the migration twice and inserts a row.

## 5. `tools/list` JSON Response

Returned shape:

```json
{
  "tools": [
    { "name": "list_artifacts", "description": "List tracked artifacts visible to this session. Returns Vec<ArtifactSummary>.", "inputSchema": { "type": "object", "properties": { "filter": { "type": "object", "properties": { "inbox": { "type": "boolean" }, "project": { "type": "string" }, "pinned": { "type": "boolean" }, "archived": { "type": "boolean" } }, "additionalProperties": false } }, "additionalProperties": false } },
    { "name": "get_artifact", "description": "Read full artifact source plus base hash, sidecar, and kind.", "inputSchema": { "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"], "additionalProperties": false } },
    { "name": "get_current_focus", "description": "Return the artifact path the user is currently viewing, or null.", "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false } },
    { "name": "get_comments", "description": "Return comments on an artifact, optionally newer than an epoch-second timestamp.", "inputSchema": { "type": "object", "properties": { "path": { "type": "string" }, "since": { "type": "integer" } }, "required": ["path"], "additionalProperties": false } },
    { "name": "get_user_messages", "description": "Return Send-back messages targeted at this session, optionally since an epoch-second timestamp.", "inputSchema": { "type": "object", "properties": { "since": { "type": "integer" } }, "additionalProperties": false } },
    { "name": "open_artifact", "description": "Foreground AgentCanvas, track the artifact if needed, and focus it in the content pane.", "inputSchema": { "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"], "additionalProperties": false } },
    { "name": "notify_user", "description": "Show a user-facing toast. Action may point at an artifact.", "inputSchema": { "type": "object", "properties": { "severity": { "type": "string", "enum": ["info", "warn", "error"] }, "message": { "type": "string" }, "action": { "type": "object", "properties": { "label": { "type": "string" }, "artifact_path": { "type": "string" } }, "required": ["label", "artifact_path"], "additionalProperties": false } }, "required": ["severity", "message"], "additionalProperties": false } },
    { "name": "attach_artifact", "description": "Mark an artifact as in-context for this agent session.", "inputSchema": { "type": "object", "properties": { "path": { "type": "string" }, "also_pin": { "type": "boolean", "default": false } }, "required": ["path"], "additionalProperties": false } },
    { "name": "add_comment", "description": "Add a comment to an artifact at a text or file-level anchor.", "inputSchema": { "type": "object", "properties": { "path": { "type": "string" }, "anchor": { "oneOf": [{ "type": "object", "properties": { "block_id": { "type": "string" }, "start_offset": { "type": "integer" }, "end_offset": { "type": "integer" } }, "required": ["start_offset", "end_offset"], "additionalProperties": false }, { "type": "object", "properties": { "kind": { "const": "file_level" } }, "required": ["kind"], "additionalProperties": false }] }, "body": { "type": "string" } }, "required": ["path", "anchor", "body"], "additionalProperties": false } }
  ]
}
```

## 6. Tests Added

- `initialize_with_valid_clientinfo_returns_serverinfo`
- `initialize_with_unknown_persona_accepts_with_warning`
- `tools_list_returns_nine_tools_with_input_schemas`
- `tools_call_stub_returns_method_not_found_for_unimplemented`
- `agent_sessions_migration_idempotent`

## 7. Verification Command Outputs

```text
orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum/crates/agent-canvas-mcp && cargo build'
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.81s
```

```text
orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum/crates/agent-canvas-app && cargo build'
Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.41s
```

```text
orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum && cargo test --bin agent-canvas-app 2>&1 | tail -80'
running 16 tests
...
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

```text
orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum/ui && ./node_modules/.bin/tsc --noEmit'
passes with no output
```

```text
cd ui && ./node_modules/.bin/vite build 2>&1 | tail -20
✓ 157 modules transformed.
dist/index.html                     0.40 kB │ gzip:   0.26 kB
dist/assets/index-DrdBowaR.css     27.76 kB │ gzip:   5.73 kB
dist/assets/index-CkpoBL1z.js   1,230.36 kB │ gzip: 393.82 kB
✓ built in 1.08s
```

The VM Vite build is blocked by the existing Rollup optional dependency state in `ui/node_modules`:

```text
Error: Cannot find module @rollup/rollup-linux-arm64-gnu
```

Host direct Vite build passes.

The end-to-end shim smoke requires AgentCanvas.app to be launched on the host so it can bind the macOS socket. This slice was verified structurally via Rust unit tests and builds; GUI launch smoke is left for the host manual pass.

## 8. `rmcp` vs Hand-Rolled Decision

I hand-rolled the JSON-RPC frame layer. The locked architecture is a custom Unix-domain socket server inside the Tauri app plus a separate stdio shim forwarding newline-delimited frames. For Slice 4, `initialize`, `tools/list`, `tools/call`, `ping`, and notification acceptance are small and direct; using `rmcp` would add adapter work without reducing the transport risk. The hand-rolled layer stays close to D8/D9 and keeps Slice 5/6 free to replace internals if a later SDK adapter becomes worthwhile.

## 9. Known Issues / Gaps

- `list_artifacts` is the only proof-of-life tool; all other tools return JSON-RPC `-32601` with `tool not yet implemented in skeleton (Slice 4)`.
- Push notifications beyond shutdown are intentionally not implemented until Slice 5.
- Coordination/write-adjacent tools remain stubs until Slice 6.
- The listener removes the socket file on graceful window close and sends shutdown notifications; process exit still owns final cleanup if the event loop is already terminating.
- Existing manual agent-panel sessions are migrated to `manual_agent_sessions` to free the required MCP `agent_sessions` table name.
