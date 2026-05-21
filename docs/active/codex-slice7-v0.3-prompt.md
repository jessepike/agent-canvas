# v0.3 Slice 7 — Agent Panel + One-Click Install + CLAUDE.md Template

You are implementing Slice 7. Read `docs/BUILD-SPEC-v0.3.md` Slice 7 (lines 207-216), Slice 4 (MCP server skeleton), and the existing agent panel + manual-session paths in `ui/src/App.tsx`.

After this slice, an external agent (Claude Code, Codex, Cursor) can be installed with one click from the AgentCanvas command palette, and AgentCanvas's live MCP sessions are visible alongside manual sessions in the agent panel.

## What "done" looks like

1. **Live MCP sessions appear in the agent panel** with `persona·agent` label, project, green-dot connected indicator, and any attached artifacts as sub-items. Manual sessions coexist (`source: "manual"`).
2. **Disconnect button** on MCP rows: emits a graceful shutdown notification to that session, removes its DB row, drops the connection. Manual sessions keep their existing Remove behavior.
3. **"Install for Claude Code" / "Install for Codex" / "Install for Cursor"** entries in the command palette write the `agent-canvas-mcp` shim path into the right config file:
   - Claude Code: `~/.claude.json` under `mcpServers.agent-canvas` (or per-project `.mcp.json` — see below)
   - Codex: `~/.codex/config.toml` under `[mcp_servers.agent-canvas]`
   - Cursor: `~/.cursor/mcp.json` under `mcpServers.agent-canvas`
   - Idempotent: if the entry already exists, replace it; preserve other entries.
4. **`docs/mcp-clients.md`** — manual install instructions for each client + the `clientInfo.agentCanvas` extension block usage + troubleshooting.
5. **`docs/claude-md-template.md`** — a paste-ready CLAUDE.md snippet telling agents when to call `open_artifact`, what to do on `notifications/artifact_updated`, how to call `add_comment`, etc.
6. **`reload_persona_registry` invalidates the MCP-side cache** so when the user reloads personas, future `initialize` calls validate against the fresh list.

## Implementation plan

### 1. Unify the agent session model

Currently `AgentSession` in `ui/src/ipc.ts` (lines 80-90) maps to `manual_agent_sessions`. The Slice 4 `agent_sessions` table holds MCP sessions but is invisible to the UI.

Extend the Rust + TS model so the panel sees both:

```ts
type AgentSession = {
  id: string;
  source: "mcp" | "manual";
  persona: string;
  agent: string;          // formerly "backbone" for manual; now unified
  project: string;        // formerly "context" for manual
  connected_at: number;
  last_active: number | null;
  // MCP-only:
  is_live: boolean;       // true when source=="mcp" and connection is open
  attached_paths: string[]; // from session_attachments
};
```

Backend command `list_agent_sessions` returns the union: manual rows + MCP rows where `disconnected_at IS NULL`. Plus archived MCP rows for the last N hours? Skip for v0.3 — only show live MCP sessions. Closed MCP sessions disappear.

Bridge the schema mismatch:
- For `manual_agent_sessions`: `agent = backbone`, `project = context`.
- For `agent_sessions` (MCP): direct columns.
- `is_live = (source == "mcp" && session_id is in the in-memory subscription map)` — derived at query time by joining with subscription state. Easy version: just rely on `disconnected_at IS NULL` from the DB.
- `attached_paths`: join `session_attachments` by session_id.

Keep `AddAgentSessionInput` for the manual-add flow. The "+ Add Agent" UI continues to work for pre-MCP setups.

### 2. Agent panel UI

Find the existing `<aside className="agent-panel">` (around line 2325 in App.tsx). Update the row render:

```tsx
<div className="agent-row" data-source={session.source}>
  <span className={`status-dot ${session.is_live ? "connected" : "offline"}`} />
  <span className="persona-chip" style={{ color: personaColors.get(session.persona) }}>
    {session.persona}·{session.agent}
  </span>
  <span className="project-label">{session.project}</span>
  {session.attached_paths.length > 0 && (
    <ul className="attached-list">
      {session.attached_paths.map(p => <li key={p}>{fileName(p)}</li>)}
    </ul>
  )}
  {session.source === "mcp"
    ? <button onClick={() => disconnectMcpSession(session.id)}>Disconnect</button>
    : <button onClick={() => removeManualSession(session.id)}>Remove</button>}
</div>
```

Manual session add UI is unchanged. MCP sessions are read-only except for Disconnect.

CSS: green dot when live (`.status-dot.connected { background: var(--accent); }`), grey when offline. Use CSS vars only — A15.

### 3. Disconnect MCP session

New Tauri command `disconnect_mcp_session(session_id: string)`:
1. Look up session in the in-memory subscription map.
2. Emit `notifications/shutdown { reason: "user_disconnect" }` to that session's tx.
3. Drop the tx (closes the writer task, the reader sees EOF on next read, cleanup runs).
4. Refresh the agent panel.

If the session doesn't exist in the map (already disconnected), just refresh — no error.

### 4. One-click install command palette entries

Three new Tauri commands:
- `install_mcp_for_claude_code() -> { config_path, action: "created" | "updated" | "noop" }`
- `install_mcp_for_codex() -> { config_path, action }`
- `install_mcp_for_cursor() -> { config_path, action }`

Each:
1. Resolve the AgentCanvas-shipped `agent-canvas-mcp` binary path. For dev, use `target/debug/agent-canvas-mcp`. For release, use the bundled binary alongside the .app. Auto-detect by checking `std::env::current_exe()` parent — pick what's robust.
2. Read existing config file. If it doesn't exist, create with empty schema.
3. Insert/replace the `agent-canvas` entry under the right key.
4. Write atomically (tmpfile + rename).
5. Return the action taken.

Config shapes:

Claude Code (`~/.claude.json`):
```json
{
  "mcpServers": {
    "agent-canvas": {
      "command": "/absolute/path/to/agent-canvas-mcp",
      "args": [],
      "env": {}
    }
  }
}
```

Codex (`~/.codex/config.toml`):
```toml
[mcp_servers.agent-canvas]
command = "/absolute/path/to/agent-canvas-mcp"
args = []
```

Cursor (`~/.cursor/mcp.json`):
```json
{
  "mcpServers": {
    "agent-canvas": {
      "command": "/absolute/path/to/agent-canvas-mcp"
    }
  }
}
```

Preserve other entries in each config. For TOML, use the `toml` crate already in workspace deps.

UI: add three command-palette entries that call the corresponding command and surface a toast with the action + config path.

### 5. Persona registry reload invalidates MCP cache

Currently `reload_persona_registry` reloads the registry for the UI. If the MCP server has its own persona cache (it should, for `initialize` validation), that cache must reload too.

Find the MCP-side persona cache in `crates/agent-canvas-app/src/mcp/`. Add a `reload_personas()` method that re-reads the registry. Wire `reload_persona_registry` Tauri command to call both the UI-side and MCP-side reloads.

If there's no MCP-side persona cache yet (Slice 4 may have skipped it), add one — read once at startup, expose `Arc<RwLock<HashSet<String>>>` of known persona names, validate against it on `initialize`, reload on demand.

### 6. Docs

`docs/mcp-clients.md` — ~150 lines:
- One section per client (Claude Code, Codex, Cursor)
- "Use the one-click install OR add this manually:" with the exact config snippet
- Brief explanation of `clientInfo.agentCanvas` extension block (persona, agent, project, session_id) and how Claude Code / Codex set these in practice
- Troubleshooting: socket not found, shim auto-launch failed, persona unknown warning

`docs/claude-md-template.md` — paste-ready snippet (~30 lines):
- "If you have AgentCanvas installed, you can use it to show artifacts to the user..."
- When to call `open_artifact(path)` (after writing an HTML/MD output the user should review)
- What to do on `notifications/artifact_updated{by:"user"}` (re-read the file via your file tools, treat the note as the next instruction)
- How to add comments via `add_comment` when annotating user work
- Example of a typical agent → user → agent round trip

## Files you will touch / create

- `crates/agent-canvas-app/src/main.rs` — `disconnect_mcp_session`, three install commands; update `list_agent_sessions` to return the union shape
- `crates/agent-canvas-app/src/mcp/sessions.rs` — query helpers for live MCP sessions + attached paths join
- `crates/agent-canvas-app/src/mcp/mod.rs` — disconnect-by-id helper; persona cache + reload
- `ui/src/ipc.ts` — updated `AgentSession` schema; `disconnectMcpSession`, three install wrappers
- `ui/src/App.tsx` — agent panel row render; command palette entries
- `ui/src/styles.css` — `.status-dot.connected`, `.attached-list` etc.
- `docs/mcp-clients.md` — new
- `docs/claude-md-template.md` — new
- `status.md`, `BACKLOG.md`

## Tests

Add to `crates/agent-canvas-app/src/mcp/mod.rs` `#[cfg(test)]`:

- `list_agent_sessions_returns_mcp_and_manual_union`
- `list_agent_sessions_includes_attached_paths`
- `list_agent_sessions_excludes_disconnected_mcp_sessions`
- `disconnect_mcp_session_emits_shutdown_and_removes_session`
- `install_for_claude_code_creates_config_when_missing`
- `install_for_claude_code_replaces_existing_entry_preserving_others`
- `install_for_codex_writes_correct_toml_shape`
- `install_for_cursor_idempotent`
- `reload_persona_registry_invalidates_mcp_cache`

Use `tempfile::TempDir` to isolate config file writes — don't touch the user's real `~/.claude.json`.

## Hard constraints

- A22 / A15 / A17 unchanged.
- Do not touch intent.md, BUILD-SPEC-v0.3.md, legacy/vellum-spec-v0.3.md.
- Manual session add flow must keep working.
- Install commands must be idempotent and preserve other config entries.
- Atomic write for config files (tmpfile + rename).
- One-click install on host writes to user's real config files — but tests must use TempDir isolation.
- Don't touch Slice 1-6 commits.
- Don't ship release work (Slice 8).

## Verification

```bash
cd crates/agent-canvas-app && cargo check -q
cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -25
cd crates/agent-canvas-mcp && cargo build
cd ui && ./node_modules/.bin/tsc --noEmit
cd ui && ./node_modules/.bin/vite build 2>&1 | tail -3
grep -rn 'allow-same-origin\|allow-modals' ui/src/ | grep -v '\.test\.' | wc -l   # 0
```

All must pass.

## Report format

Write `docs/active/codex-slice7-v0.3-report-2026-05-21.md`:

1. Files modified / created
2. Unified AgentSession shape (Rust + TS, side by side)
3. Three install command bodies (paste the TOML / JSON they write)
4. Persona cache reload wiring
5. Tests added
6. Verification output
7. Known issues / gaps

Commit:

```
feat(v0.3-slice7): agent panel MCP integration, one-click install, CLAUDE.md template
```

Single atomic commit.

## Out of scope (do NOT build)

- Release work (Slice 8 — versions, README refresh, tag)
- Per-session permission scopes
- Auth tokens / api keys for MCP (skip; local-only socket)
- Multiple-window / multi-vault support
- Cross-machine sync of agent_sessions
- Bundled-shim packaging optimization
- Notarization

If you notice an improvement adjacent, write to `BACKLOG.md` with `[v0.3-slice7-spinoff]`. Do not build it.
