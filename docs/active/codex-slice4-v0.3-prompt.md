# v0.3 Slice 4 — MCP Server Skeleton

You are implementing Slice 4 of AgentCanvas v0.3. Read `docs/BUILD-SPEC-v0.3.md`, especially Slice 4 (lines 175-185), decisions D8 / D9 / D10 / D12, and the 8-tool surface (lines 106-127).

This is **skeleton only** — establish the protocol surface, NOT the tool implementations. Slice 5 wires real read tools; Slice 6 wires coordination tools. Get the plumbing right first.

## What "done" looks like

Three outcomes, all verifiable from a terminal:

1. **AgentCanvas binds `~/Library/Application Support/AgentCanvas/mcp.sock` on startup** and listens for newline-delimited JSON-RPC 2.0 frames. Stale socket file is unlinked before bind.
2. **A separate `agent-canvas-mcp` binary** (the stdio shim) connects to that socket and forwards stdio JSON-RPC ↔ socket JSON-RPC transparently.
3. **`initialize` + `tools/list`** work end-to-end. Send `{"jsonrpc":"2.0","id":1,"method":"initialize","params":{...}}` through the shim, get back capabilities + serverInfo. Send `tools/list` next, get back the 8-tool schema list. All 8 tool methods exist as stubs returning a "not implemented in skeleton" JSON-RPC error.

The user must be able to verify with:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"manual-test","version":"0.0.1","agentCanvas":{"persona":"cpo","agent":"claude","project":"agent-canvas","session_id":"manual-test-1"}}}}' | ./target/debug/agent-canvas-mcp
```

…and see a JSON-RPC response with `serverInfo.name = "AgentCanvas"` plus capabilities.

## Locked architecture (do not deviate)

- **Transport (D8):** Unix domain socket at `~/Library/Application Support/AgentCanvas/mcp.sock`. Shim is a separate binary. Newline-delimited JSON-RPC 2.0.
- **Auto-launch (D9):** On bind failure (socket missing / app not running), shim invokes `open -a AgentCanvas.app`, polls socket for up to 5s, then connects. Clean MCP error response if launch never produces a socket.
- **Session identity (D10):** `(persona, agent, project, session_id)`. Declared on `initialize` via `clientInfo.agentCanvas` extension block. Validates persona against the runtime registry, defaults gracefully if unknown. Stores a row in a new `agent_sessions` SQLite table.
- **No agent write surface (D14):** The 8 tools are read + coordination. No `create_artifact` / `commit_edit` / `propose_edit`.

## Implementation plan

### 1. New workspace crate `crates/agent-canvas-mcp`

```
crates/agent-canvas-mcp/
├── Cargo.toml
└── src/
    └── main.rs
```

Binary target: `agent-canvas-mcp`. Single file is fine for the shim — it's small.

Behavior:
1. Compute socket path: `~/Library/Application Support/AgentCanvas/mcp.sock`. (Use `dirs::home_dir()` or `std::env::var("HOME")`.)
2. Attempt `UnixStream::connect(socket)`. On success, enter the bidirectional copy loop (below).
3. On failure (ENOENT / ECONNREFUSED): `Command::new("open").args(["-a", "AgentCanvas.app"]).spawn()`. Poll the socket path with 200ms sleep up to 5s.  If still no connection, write a clean MCP error response and exit with code 2.
4. Bidirectional copy: spawn one tokio task that reads stdin lines → writes socket. Another task that reads socket lines → writes stdout. Either task exiting causes both to exit.
5. Use tokio for async IO; lean on `tokio::io::BufReader` line-reading on both sides.

Add `dirs = "5"` to workspace deps if convenient. If you don't want a new dep, use `env::var("HOME")?` manually.

### 2. Tauri-side server (`crates/agent-canvas-app/src/main.rs`)

Add a new module file `crates/agent-canvas-app/src/mcp/mod.rs` (and break into submodules if it gets >300 lines). Keep `main.rs` clean — only the `init_mcp_server(app_handle)` call lives there.

On `tauri::Builder::default().setup(...)`:
- Compute socket path.
- `fs::remove_file(&socket_path).ok();` to clear stale socket.
- Ensure parent dir exists.
- Spawn a tokio task on the existing runtime (or create one) that:
  1. `UnixListener::bind(&socket_path)`.
  2. Loop `accept()`. For each connection, spawn a per-connection handler task.
- Per-connection handler:
  1. Wraps the `UnixStream` in `BufReader` + `BufWriter`.
  2. Reads one JSON-RPC frame per line.
  3. Dispatches by method; writes response (or error) as a single line.
  4. On EOF / error: log + exit the task.

Handler must support these methods at the protocol level (return stub responses for unimplemented tool calls):

- `initialize` — store session in `agent_sessions`, return `{ protocolVersion, capabilities, serverInfo: { name: "AgentCanvas", version: "0.3.0" } }`. Capabilities should declare `tools: {}` so clients know we support tools.
- `tools/list` — return the 8-tool list (see "Tool schemas" below).
- `tools/call` — dispatch on `name` field. For Slice 4, every tool returns a JSON-RPC error `code: -32601, message: "tool not yet implemented in skeleton (Slice 4)"` EXCEPT `list_artifacts` which can return a minimal stub `Vec<ArtifactSummary>` reading from the existing DB (proof of plumbing). Implement that one minimal proof-of-life tool so an agent can sanity-check the bridge works.
- `notifications/initialized` — accept silently.
- `ping` — return `{}`.

Unknown method: standard JSON-RPC `code: -32601`.

Bad JSON: standard `code: -32700`.

### 3. New SQLite table `agent_sessions`

Idempotent migration (same `PRAGMA table_info` guard pattern used in Slice 1):

```sql
CREATE TABLE IF NOT EXISTS agent_sessions (
  session_id      TEXT NOT NULL,
  source          TEXT NOT NULL,           -- "mcp" or "manual"
  persona         TEXT NOT NULL,
  agent           TEXT NOT NULL,
  project         TEXT NOT NULL,
  connected_at    INTEGER NOT NULL,        -- epoch seconds
  disconnected_at INTEGER,                 -- null while live
  PRIMARY KEY (session_id, connected_at)
);
```

On `initialize`: insert row. On connection close: set `disconnected_at = now()`.

### 4. Persona resolution

Read existing persona registry helper. Validate `clientInfo.agentCanvas.persona` against it.  If unknown, store as-given but mark the session row with the unknown persona — return the session ID either way (do not reject). Surface "unknown persona" as a one-time `tracing::warn!`.

### 5. Tool schemas (in `tools/list` response)

Return an array of 8 `Tool` objects matching the MCP spec shape:

```json
{
  "name": "list_artifacts",
  "description": "...",
  "inputSchema": { "type": "object", "properties": { ... }, "additionalProperties": false }
}
```

Use the args / returns from `BUILD-SPEC-v0.3.md` lines 110-118 verbatim. Don't invent extra params. Hand-write the JSON schemas — keep them tight. `inputSchema` is required for MCP; outputs are not part of the spec but include a `description` for each.

Tools (just names, see spec for full args):

1. `list_artifacts`
2. `get_artifact`
3. `get_current_focus`
4. `get_comments`
5. `get_user_messages`
6. `open_artifact`
7. `notify_user`
8. `attach_artifact`
9. `add_comment`

(That's 9, but the spec table calls it the 8-tool surface plus `add_comment` at the bottom — include all 9.)

### 6. Graceful shutdown

Tauri `on_window_event` for `WindowEvent::CloseRequested` (main window) or app exit: emit `notifications/shutdown { }` to all open connections, sleep ~150ms, drop all listeners, `remove_file` the socket.

## Files you will touch / create

- `Cargo.toml` — add `agent-canvas-mcp` to workspace members; add `dirs = "5"` to workspace deps if used
- `crates/agent-canvas-mcp/Cargo.toml` — new
- `crates/agent-canvas-mcp/src/main.rs` — new, the stdio shim
- `crates/agent-canvas-app/Cargo.toml` — depend on `tokio` (already there) and any rmcp crate version you need
- `crates/agent-canvas-app/src/main.rs` — call `init_mcp_server(&app_handle)` in setup; window close hook
- `crates/agent-canvas-app/src/mcp/mod.rs` — new module: listener, dispatcher, handlers
- `crates/agent-canvas-app/src/mcp/tools.rs` — new: tool schema list + dispatcher stub
- `crates/agent-canvas-app/src/mcp/sessions.rs` — new: session table CRUD + `agent_sessions` migration
- `status.md` — Slice 4 session log
- `BACKLOG.md` — close Slice 4, surface spinoffs

## Verification you must run

```bash
# Build everything
cd crates/agent-canvas-mcp && cargo build
cd crates/agent-canvas-app && cargo build

# Rust unit tests
cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -20

# Frontend (unchanged)
cd ui && ./node_modules/.bin/tsc --noEmit
cd ui && ./node_modules/.bin/vite build 2>&1 | tail -5

# Plumbing smoke test (start app first, then in another terminal):
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.0.1","agentCanvas":{"persona":"cpo","agent":"claude","project":"agent-canvas","session_id":"smoke-1"}}}}' | ./target/debug/agent-canvas-mcp
```

Expected smoke response includes `"serverInfo":{"name":"AgentCanvas"...}`.

You don't need to launch the GUI app from inside the dev VM — the user verifies that on host. Your job: it builds, tests pass, the protocol surface is structurally correct.

Add at least these unit tests (in `crates/agent-canvas-app/src/mcp/mod.rs` `#[cfg(test)]`):

- `initialize_with_valid_clientinfo_returns_serverinfo`
- `initialize_with_unknown_persona_accepts_with_warning` (assert session row inserted)
- `tools_list_returns_nine_tools_with_input_schemas`
- `tools_call_stub_returns_method_not_found_for_unimplemented`
- `agent_sessions_migration_idempotent` (run migration twice, no error)

## Hard constraints

- A22 sandbox flags unchanged.
- A15 / A17 unchanged.
- Do not modify `intent.md`, `legacy/vellum-spec-v0.3.md`, `BUILD-SPEC-v0.3.md`.
- Do not modify Slice 1/2/3 comment / sidecar / viewer code paths.
- Do not implement notifications wire (Slice 5).
- Do not implement Send-back routing (Slice 6).
- Socket path is fixed: `~/Library/Application Support/AgentCanvas/mcp.sock`. Use it on macOS unconditionally for v0.3. (Other OSes deferred.)
- Use `rmcp` crate (already in workspace deps) where it saves real time. If `rmcp`'s API doesn't fit the unix-socket + custom-bridge model cleanly, roll the JSON-RPC frame layer by hand — it's only ~80 lines. Justify the choice in the report.

## Report format

Write `docs/active/codex-slice4-v0.3-report-2026-05-20.md` with:

1. Files modified / created
2. Socket bind / unlink behavior — paste the actual init code
3. Stdio shim auto-launch flow — paste the auto-launch block
4. `agent_sessions` migration SQL + idempotency check
5. `tools/list` JSON response (paste the actual returned shape, formatted)
6. Tests added
7. Verification command outputs
8. rmcp-vs-hand-rolled decision + rationale
9. Known issues / gaps

Commit message:

```
feat(v0.3-slice4): MCP server skeleton, stdio shim, initialize, tools/list
```

Single atomic commit.

## Out of scope (do NOT build)

- Real implementations for any tool except `list_artifacts` (minimal stub for proof-of-life)
- `notifications/artifact_updated` / `artifact_focused` push (Slice 5)
- `user_messages` table or Send-back wiring (Slice 6)
- Agent panel UI updates (Slice 7)
- One-click install for Claude Code / Codex / Cursor (Slice 7)
- CLAUDE.md template (Slice 7)
- Comment-author = `persona·agent` UI rendering (touchup in Slice 6 or 7)

If you notice an improvement adjacent to this slice, write it to `BACKLOG.md` with a `[v0.3-slice4-spinoff]` tag. Do not build it.
