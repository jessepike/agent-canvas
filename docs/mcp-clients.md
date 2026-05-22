# AgentCanvas MCP Clients

AgentCanvas exposes a local MCP server through the shipped `agent-canvas-mcp` shim. The desktop app owns the Unix socket and the shim bridges stdio MCP clients to that socket.

Use the command palette entries when possible:

- `Install for Claude Code`
- `Install for Codex`
- `Install for Cursor`

The installers are idempotent. They replace only the `agent-canvas` MCP entry and preserve other configured servers.

## Claude Code

Use the one-click install OR add this manually to `~/.claude.json`:

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

For a per-project install, put the same server entry in the project `.mcp.json`:

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

Claude Code cannot set `clientInfo.agentCanvas` itself, so **the `agent-canvas-mcp` shim injects it on `initialize`**. Without any configuration the shim derives:

- `persona` → `default`
- `agent` → `claude`
- `project` → basename of the working directory
- `session_id` → a unique per-process id (so each connection is distinct and cleanly disconnectable — no more stacked `unknown-session` ghost cards)

To override any field, set environment variables in the server's `env` block:

```json
{
  "mcpServers": {
    "agent-canvas": {
      "command": "/absolute/path/to/agent-canvas-mcp",
      "args": [],
      "env": {
        "AGENTCANVAS_PERSONA": "cpo",
        "AGENTCANVAS_PROJECT": "agent-canvas",
        "AGENTCANVAS_SESSION_ID": "claude-agent-canvas-1"
      }
    }
  }
}
```

Set `AGENTCANVAS_PERSONA` to the persona that best matches the project agent. Set a stable `AGENTCANVAS_SESSION_ID` only if you want reconnects to replace the prior card instead of generating a fresh per-process id; concurrent sessions must use distinct ids. A client that *can* set `clientInfo.agentCanvas` directly takes precedence — the shim never overrides an explicit block.

## Codex

Use the one-click install OR add this manually to `~/.codex/config.toml`:

```toml
[mcp_servers.agent-canvas]
command = "/absolute/path/to/agent-canvas-mcp"
args = []
```

Codex-compatible MCP launches should include the same AgentCanvas extension block during `initialize`:

```json
{
  "clientInfo": {
    "name": "codex",
    "version": "local",
    "agentCanvas": {
      "persona": "cto",
      "agent": "codex",
      "project": "agent-canvas",
      "session_id": "codex-unique-session"
    }
  }
}
```

If a client cannot customize `clientInfo`, the `agent-canvas-mcp` shim injects identity for it (see the Claude Code section — `persona: "default"`, `agent: "claude"`, `project:` working-dir basename, and a unique per-process `session_id`). Set the `AGENTCANVAS_*` env vars to make the agent panel more useful. The legacy `unknown-session` fallback only applies to a direct socket client that bypasses the shim.

## Cursor

Use the one-click install OR add this manually to `~/.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "agent-canvas": {
      "command": "/absolute/path/to/agent-canvas-mcp"
    }
  }
}
```

Cursor MCP sessions should use `clientInfo.agentCanvas` the same way:

```json
{
  "clientInfo": {
    "name": "cursor",
    "version": "local",
    "agentCanvas": {
      "persona": "cto",
      "agent": "cursor",
      "project": "agent-canvas",
      "session_id": "cursor-unique-session"
    }
  }
}
```

## Extension Block

`clientInfo.agentCanvas` is an AgentCanvas-specific extension. It does not change MCP protocol semantics; it gives AgentCanvas enough metadata to show the live session and scope artifacts.

Fields:

- `persona`: Agent persona name. AgentCanvas validates against the persona registry and logs a warning for unknown names.
- `agent`: Client or model family label, such as `claude`, `codex`, or `cursor`.
- `project`: Human-readable project or workspace label.
- `session_id`: Unique id for this running agent session.

Live sessions appear in the Agent panel as `persona·agent`, with project and attached artifacts. Calls to `open_artifact` and `attach_artifact` associate files with that session.

## Troubleshooting

### Socket Not Found

The desktop app must be running before the shim can connect. Launch AgentCanvas, then restart the MCP client.

Expected socket:

```text
~/Library/Application Support/AgentCanvas/mcp.sock
```

### Shim Auto-Launch Failed

The config must point at the absolute `agent-canvas-mcp` binary. In development this is usually:

```text
target/debug/agent-canvas-mcp
```

Run `cargo build` in `crates/agent-canvas-mcp` if the binary is missing.

### Persona Unknown Warning

AgentCanvas accepts unknown personas but logs a warning and shows the raw name. Use `Reload Persona Registry` from the command palette after editing personas. Future MCP `initialize` calls validate against the refreshed list.

### Session Does Not Appear

Confirm the MCP client sent `initialize` and included a distinct `clientInfo.agentCanvas.session_id`. Closed sessions disappear from the Agent panel.
