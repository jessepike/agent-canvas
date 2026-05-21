# Codex Slice 7 v0.3 Report — 2026-05-21

## 1. Files Modified / Created

Modified:

- `BACKLOG.md`
- `Cargo.lock`
- `crates/agent-canvas-app/Cargo.toml`
- `crates/agent-canvas-app/src/main.rs`
- `crates/agent-canvas-app/src/mcp/mod.rs`
- `crates/agent-canvas-app/src/mcp/sessions.rs`
- `status.md`
- `ui/src/App.tsx`
- `ui/src/ipc.ts`
- `ui/src/styles.css`

Created:

- `docs/mcp-clients.md`
- `docs/claude-md-template.md`
- `docs/active/codex-slice7-v0.3-report-2026-05-21.md`

## 2. Unified AgentSession Shape

Rust:

```rust
pub struct AgentSession {
    pub id: String,
    pub source: String,
    pub persona: String,
    pub agent: String,
    pub project: String,
    pub connected_at: i64,
    pub last_active: Option<i64>,
    pub is_live: bool,
    pub attached_paths: Vec<String>,
}
```

TypeScript:

```ts
export const AgentSession = z.object({
  id: z.string(),
  source: z.enum(["mcp", "manual"]),
  persona: z.string(),
  agent: z.string(),
  project: z.string(),
  connected_at: z.number(),
  last_active: z.number().nullable(),
  is_live: z.boolean(),
  attached_paths: z.array(z.string())
}).strict();
```

Manual rows map `agent = backbone` and `project = context`. MCP rows come from `agent_sessions` with `disconnected_at IS NULL`; `attached_paths` comes from `session_attachments`.

## 3. Install Command Bodies

Claude Code writes `~/.claude.json`:

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

Codex writes `~/.codex/config.toml`:

```toml
[mcp_servers.agent-canvas]
command = "/absolute/path/to/agent-canvas-mcp"
args = []
```

Cursor writes `~/.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "agent-canvas": {
      "command": "/absolute/path/to/agent-canvas-mcp"
    }
  }
}
```

Each installer reads the existing config, preserves unrelated entries, replaces `agent-canvas`, and writes through a temp file plus rename.

## 4. Persona Cache Reload Wiring

`mcp::init_mcp_server` seeds an MCP-side `Arc<RwLock<HashSet<String>>>` from the configured persona registry. `initialize` validates against that cache.

`reload_persona_registry` still refreshes the UI/database registry, and now also calls:

```rust
mcp::reload_personas(paths.persona_registry.clone()).await;
```

Future MCP `initialize` calls validate against the refreshed persona set.

## 5. Tests Added

Added MCP/app tests:

- `list_agent_sessions_returns_mcp_and_manual_union`
- `list_agent_sessions_includes_attached_paths`
- `list_agent_sessions_excludes_disconnected_mcp_sessions`
- `disconnect_mcp_session_emits_shutdown_and_removes_session`
- `install_for_claude_code_creates_config_when_missing`
- `install_for_claude_code_replaces_existing_entry_preserving_others`
- `install_for_codex_writes_correct_toml_shape`
- `install_for_cursor_idempotent`
- `reload_persona_registry_invalidates_mcp_cache`

## 6. Verification Output

```text
cd crates/agent-canvas-app && cargo check -q
passes with pre-existing ts-rs sidecar warning
```

```text
cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -25
test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.47s
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
✓ built in 1.13s
```

```text
grep -rn 'allow-same-origin\|allow-modals' ui/src/ | grep -v '\.test\.' | wc -l
0
```

## 7. Known Issues / Gaps

- Vite still emits the known chunk-size warning.
- `cargo check -q` still emits the pre-existing `ts-rs` warning for the sidecar serde attribute.
