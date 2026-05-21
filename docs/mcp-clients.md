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

Claude Code should identify the live session during `initialize` with the `clientInfo.agentCanvas` extension block:

```json
{
  "clientInfo": {
    "name": "claude-code",
    "version": "local",
    "agentCanvas": {
      "persona": "cpo",
      "agent": "claude",
      "project": "agent-canvas",
      "session_id": "claude-code-unique-session"
    }
  }
}
```

Use the persona that best matches the project agent. `session_id` should be stable for the running agent process and unique across concurrent sessions.

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

If the client cannot customize `clientInfo`, AgentCanvas still accepts the connection and falls back to `persona: "default"`, `agent: "unknown"`, `project: "default"`, and `session_id: "unknown-session"`. Customized client info is strongly preferred because it makes the agent panel useful.

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
