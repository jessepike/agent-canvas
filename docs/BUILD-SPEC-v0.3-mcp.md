# AgentCanvas v0.3 — MCP Server Build Spec

**Goal:** ship the live MCP integration that was deferred from v0. The last item on the v0 "Out of Scope" list.
**Scope:** an MCP server inside AgentCanvas + a thin stdio shim that agents launch + the 10-tool surface from the original v0 plan, calibrated for what v0.1–v0.2 actually built.
**Build window:** ~6–10 Codex hours across 7 atomic slices.
**Owner:** Jesse. **Orchestrator:** Claude (CPO). **Implementer:** Codex.
**Trigger:** v0.2.2 stable. Pasteboard handoff still works as fallback throughout — MCP is additive, not replacement.

---

## Reads-Required (every slice)

1. This file (`docs/BUILD-SPEC-v0.3-mcp.md`)
2. `intent.md` — product north star (note v2.0 explicitly dropped MCP client / `rmcp` from the OLD Vellum framing; v0.3 reintroduces MCP under a different shape)
3. `CLAUDE.md` — non-negotiable invariants (A12–A17 still apply)
4. `BUILD-SPEC-v0.md` and `docs/BUILD-SPEC-v0.2-finish.md` — what already exists
5. `crates/agent-canvas-app/src/main.rs` — existing Tauri commands map 1:1 onto MCP tools where possible
6. `crates/vellum-core/src/sidecar/mod.rs` — sidecar schema (IdentityMap, BaseSnapshot, Comment)
7. `ui/src/App.tsx` — existing agent panel, persona registry, review-state machine

---

## Architecture Decisions (locked before build starts)

### D1 — Transport: stdio shim + unix domain socket

- AgentCanvas (running Tauri app) owns a unix domain socket at
  `~/Library/Application Support/AgentCanvas/mcp.sock`.
- An `agent-canvas-mcp` binary (separate Cargo bin in the workspace) implements
  the **standard MCP stdio protocol** and forwards JSON-RPC frames bidirectionally
  to the socket. Agents launch this binary via their MCP config; they speak
  stdio MCP as they normally would.
- Rationale: standard MCP clients (Claude Desktop, Claude Code, Codex MCP) use
  stdio. The shim makes AgentCanvas drop-in compatible. The socket gives the
  running Tauri app live state access (current focus, review state, sidecar
  writes that the UI sees instantly).
- If AgentCanvas isn't running when the shim launches: shim returns a clear
  protocol error to the agent (`AGENTCANVAS_NOT_RUNNING`). The shim does **NOT**
  auto-launch the GUI in v0.3 (avoids surprise app launches).

### D2 — Session identity = (persona, agent, project) declared on `initialize`

Standard MCP `initialize` is extended with an AgentCanvas-specific
`clientInfo.agentCanvas` block:

```json
{
  "clientInfo": {
    "name": "claude-code",
    "version": "...",
    "agentCanvas": {
      "persona": "cpo",
      "agent": "claude",
      "project": "agent-canvas"
    }
  }
}
```

- `persona` must match the persona registry (cpo, cto, cfo, cmo, ciso, cro, krypton, …). Unknown → `default` persona.
- `agent` is informational (claude / codex / cursor / etc.). Used for the agent panel label.
- `project` is optional. If present, MUST be a valid project name (no path traversal, no slashes). Missing → session is "Inbox-only" scope.

The server stores the connection in an in-memory `agent_sessions` map keyed by
session-id. The existing SQLite `agent_sessions` table is reused for persistence
across app restarts (existing manual-add entries coexist; MCP-originated ones
get a `source = "mcp"` flag).

### D3 — Trust model: project scoping, propose-edit gated

Each session sees:

- All Inbox files (read + propose-edit + comment + notify + attach).
- All files in the session's declared `project` folder (same rights).
- All Pinned files (read-only — pin is a user-attention signal, not an
  edit-scope grant).
- Archive: **invisible** unless the agent passes `include_archive: true` on
  `list_artifacts` AND was launched with a permissive trust flag (deferred to v0.4).
- Other projects: invisible.

**Propose-edit is the default write path.** `commit_edit` (direct write) is
gated behind an explicit user approval step in v0.3 (a "Auto-commit edits from
this agent" toggle in the agent panel, defaulting OFF). Without that toggle,
`commit_edit` returns `EDIT_REQUIRES_USER_APPROVAL` and the agent should fall
back to `propose_edit`.

### D4 — Concurrency: multiple sessions, last-write-wins on comments, hash-guarded on edits

- Multiple sessions can be live at once. Each gets its own MCP session-id.
- Comment writes are append-only into the sidecar; concurrent appends from
  different agents are merge-safe (UUID-keyed comment array).
- `propose_edit` / `commit_edit` carry a `base_hash` (current file source hash
  the agent last read). Server rejects if disk has moved past that hash (same
  optimistic concurrency model as `save_document`).

### D5 — Lifecycle: server starts with app, dies with app

- Tauri `setup` spawns an async task that binds the socket and runs the server
  for the app's lifetime.
- On graceful quit (Cmd+Q): server sends `notification/shutdown` to all
  connected sessions, then unbinds the socket. The Application-Support
  directory is canonical; the socket file is `unlink`ed on bind to handle
  stale socket files from a hard crash.
- The shim binary detects socket disappearance and returns a clean MCP error
  to its agent client (`AGENTCANVAS_DISCONNECTED`). Agents can re-invoke later.

### D6 — Schema versioning

- MCP `initialize` `serverInfo.version` carries the AgentCanvas MCP protocol
  version (`"agentcanvas-mcp/1.0.0"`).
- Sidecar schema gains a `mcp_schema_version` field for forward migration.
- Breaking protocol changes ship under `v2/` tools (additive: old tools stay
  during a deprecation window).

### D7 — Out of scope for v0.3 (explicit deferrals to v0.4+)

- `attach_artifact` cross-machine sync (current scope: same machine only).
- Encrypted at-rest sidecar (current scope: plain JSON on disk, trusts macOS
  user-level filesystem permissions).
- Per-artifact agent visibility (every project-scoped agent sees all files in
  the project). Per-artifact trust ladders deferred.
- Auto-launch AgentCanvas from shim when app isn't running.
- MCP-driven persona color updates (the registry stays in `pike-agents/plugins/`).

---

## Architecture Invariants

A12–A17 from `BUILD-SPEC-v0.2-finish.md` still apply. Adding:

- **A18 — Stdio shim binary path-bounds nothing on its own.** All
  path-bounding lives in the Tauri-side MCP server using
  `path_within_canvas`. The shim only forwards bytes.
- **A19 — Persona declared on `initialize` must validate against the
  registry.** Unknown personas are accepted but tagged `default`; agent
  panel shows them as `unknown · {agent}`. Never reject a connection on
  unknown persona (graceful degradation).
- **A20 — Every MCP write goes through the same Rust commands the UI uses.**
  No parallel "MCP-only" file-write path. The MCP server is a *client* of the
  same internal command surface; this keeps a single source of truth for
  validation, optimistic concurrency, and sidecar shape.
- **A21 — `notify_user` and `attach_artifact` push into existing UI state.**
  Reusing the toast + selection mechanisms — no new UI surface for MCP-driven
  events.

---

## Tool Surface (10 tools, v1)

| Tool | Direction | Scope | Notes |
|------|-----------|-------|-------|
| `list_artifacts` | read | session-scoped | Returns `Vec<ArtifactSummary>` (path, name, persona, review_state, mtime, pinned). Args: `{ folder?: "inbox"|"project"|"pinned", limit?, after? }`. |
| `get_artifact` | read | session-scoped | Returns source + base_hash + sidecar (comments + identity_map + base_snapshot). Args: `{ path }`. |
| `get_current_focus` | read | global | Returns the file currently displayed in the AgentCanvas content pane, or `null` if none. Lets agents react to user attention. |
| `get_comments` | read | session-scoped | Returns comments for an artifact. Args: `{ path }`. |
| `get_user_messages` | read | session-scoped | Returns user-typed messages targeted at this agent's persona (Send-to-Agent notes). Args: `{ since? }`. |
| `add_comment` | write | session-scoped | Appends a comment to the sidecar. Args: `{ path, anchor, body }`. Returns the new Comment. |
| `propose_edit` | write | session-scoped | Stores a proposed patch on the artifact (review_state → `needs-work`, comment with diff). User sees it in the UI as a pending review. Args: `{ path, base_hash, patch_unified_diff | new_source, message? }`. |
| `commit_edit` | write | session-scoped, gated | Direct atomic write (same as save_document). Returns `EDIT_REQUIRES_USER_APPROVAL` unless the agent has been granted auto-commit by the user. Args: `{ path, base_hash, new_source }`. |
| `notify_user` | write | global | Pushes a toast in the AgentCanvas UI. Args: `{ severity: "info"|"warn"|"error", message, action? }`. |
| `attach_artifact` | write | session-scoped | Marks an artifact as "in-context for this agent" — pins it in the agent panel and (optionally) auto-toggles the user-visible pin star. Args: `{ path, also_pin?: bool }`. |

All tools return JSON-RPC standard error shape on failure. Error codes are
documented in slice 1.

---

## Slice 1 — Server skeleton + initialize handshake (~90 min)

### 1a. New Cargo binary `agent-canvas-mcp`

Workspace member at `crates/agent-canvas-mcp/`. Single binary. Reads JSON-RPC
frames from stdin, writes to stdout, forwards to/from the unix socket at
`~/Library/Application Support/AgentCanvas/mcp.sock`.

If the socket doesn't exist OR connection fails: emit one JSON-RPC error
response per pending request with code `-32000` and message
`"AGENTCANVAS_NOT_RUNNING: launch AgentCanvas.app, then retry."`. Then exit
with code 1 if no requests arrived within 30s of startup; otherwise stay alive
and re-attempt connection on next request (with backoff).

### 1b. Tauri-side server task

In `crates/agent-canvas-app/src/main.rs`, add a `setup` step that spawns a
tokio task binding the unix socket. On bind:
- `unlink` any stale socket file.
- Listen on `~/Library/Application Support/AgentCanvas/mcp.sock`.
- For each accepted connection, spawn a per-session handler task.

Frame format: JSON-RPC 2.0 over newline-delimited JSON. No content-length
prefix (matches what Claude Desktop expects via stdio shim).

### 1c. `initialize` handshake

Implement `initialize` → returns `serverInfo: { name: "AgentCanvas", version: "0.3.0", protocolVersion: "2024-11-05", agentCanvas: { schema: "1.0.0" } }` and `capabilities: { tools: { listChanged: false } }`.

The handler:
1. Validates `clientInfo.agentCanvas.persona` against the persona registry; tags `default` if unknown (logs a warning, does NOT reject).
2. Validates `clientInfo.agentCanvas.project` if present (uses `safe_project_segment`).
3. Inserts/updates the SQLite `agent_sessions` row with `source = "mcp"`, `persona`, `agent`, `project`, `session_id`.
4. Returns a `session_id` in the response.

### 1d. `tools/list` returns the 10-tool surface

Returns name + input schema for each of the 10 tools above, with proper JSON
Schema. No tool execution yet — that's slice 2+.

### 1e. Graceful shutdown

`tauri::AppHandle::on_event(WindowEvent::CloseRequested)` and explicit Cmd+Q
flow notifies all active sessions with a `notification/shutdown`, then unbinds
the socket. Test: launch shim, send `initialize`, quit app, shim sees clean
disconnect.

### Acceptance for Slice 1

- `cargo build -p agent-canvas-mcp` produces a binary.
- With AgentCanvas running, `agent-canvas-mcp` accepts a JSON-RPC `initialize`
  on stdin and returns a valid response within 200ms.
- With AgentCanvas NOT running, `agent-canvas-mcp` returns
  `AGENTCANVAS_NOT_RUNNING` cleanly.
- `tools/list` returns 10 entries.

**Commit:** `feat(mcp): server skeleton, stdio shim, initialize handshake, tools list`

---

## Slice 2 — Read tools (~75 min)

Implement `list_artifacts`, `get_artifact`, `get_current_focus`,
`get_comments`, `get_user_messages`.

### 2a. Read-tool implementations

Each tool maps onto an existing internal Rust function:

| Tool | Maps to |
|------|---------|
| `list_artifacts` | scoped wrapper around `list_files_under` + `list_pinned` |
| `get_artifact` | `open_document` + `load_sidecar` |
| `get_current_focus` | reads from a new `app_focus` field in `AppState` updated by the UI on every artifact selection (slice 2b) |
| `get_comments` | reads `comments` array from sidecar |
| `get_user_messages` | reads from a new `user_messages` SQLite table — populated when the user clicks Send-to-Agent on a target persona, capturing the note + action verb + selected artifact path |

### 2b. UI write to `app_focus`

`App.tsx` on `setArtifact(...)`: fire a new IPC command
`update_app_focus(path | null)`. The Rust server reads this for
`get_current_focus`.

### 2c. `user_messages` table

Migration: `CREATE TABLE IF NOT EXISTS user_messages (id INTEGER PRIMARY KEY, persona TEXT, action_verb TEXT, artifact_path TEXT, note TEXT, created_at INTEGER)`. On Send-to-Agent, INSERT a row with the selected agent's persona. `get_user_messages` returns rows where `persona = session.persona AND created_at > since`.

### 2d. Session scoping enforcement

All read tools accept a session-id (looked up from the connection). Path
arguments are validated:
- Must be `path_within_canvas`.
- Must be within the session's scope (Inbox + assigned project + Pinned).
- Reject with `PATH_OUT_OF_SCOPE` if violated.

### Acceptance for Slice 2

- A test MCP client (use `agent-canvas-mcp` directly with a scripted stdin)
  can call `initialize` then `list_artifacts` and get the current Inbox.
- `get_current_focus` returns the file the user is looking at (verify with
  manual UI test: open file → MCP call returns its path).
- `get_user_messages` returns only messages for the calling persona.

**Commit:** `feat(mcp): read tools — list_artifacts, get_artifact, get_current_focus, get_comments, get_user_messages`

---

## Slice 3 — Notify + attach + add_comment (~60 min)

### 3a. `notify_user`

Tool args: `{ severity, message, action? }`. The Rust handler emits a Tauri
event `mcp://notify`. `App.tsx` listens and pushes a toast styled by severity
(reuses existing `handoffToast` mechanism, gains color variants).

If `action` is present (e.g., `{ label: "View", artifact_path: "..." }`), the
toast renders a clickable action button. On click, the UI selects + opens the
artifact path (path-bounded as always).

### 3b. `attach_artifact`

Tool args: `{ path, also_pin?: bool }`. Inserts a row in a new
`agent_attachments` table `(session_id, artifact_path, attached_at)`. The
agent panel renders attached artifacts under each agent session as
sub-items. If `also_pin = true`, calls `toggle_pin` to set the user-visible
pin star.

### 3c. `add_comment`

Args: `{ path, anchor: { block_id?, start_offset, end_offset }, body }`.
Calls `update_sidecar_comments` with the existing comments array + the new
Comment. The agent's persona is recorded as the comment author.

### Acceptance for Slice 3

- A scripted MCP call to `notify_user` shows a toast in the running app
  within 250ms.
- A scripted `attach_artifact` adds a row to the agent panel.
- A scripted `add_comment` appears in the comments panel on the next sidecar
  reload.

**Commit:** `feat(mcp): notify_user, attach_artifact, add_comment`

---

## Slice 4 — Edit tools (~90 min)

### 4a. `propose_edit`

The proposed-edit primitive: agent suggests a change without writing the file.

Args: `{ path, base_hash, patch_unified_diff?, new_source?, message? }` —
exactly one of `patch_unified_diff` or `new_source` required.

Behavior:
1. Validate `base_hash` matches current disk state. Reject with
   `BASE_HASH_MISMATCH` if not.
2. Compute the resulting new source (apply patch, or use `new_source`
   directly).
3. Store the proposal in a new `proposed_edits` SQLite table:
   `(id, path, session_id, persona, base_hash, new_source, message, created_at, status TEXT DEFAULT 'pending')`.
4. Set `review_state = "needs-work"` on the artifact.
5. Add a synthetic comment at offset 0 with body
   `"Edit proposed by {persona}: {message}"` and a `proposed_edit_id`
   anchor field.
6. Return the proposal id.

UI (`App.tsx`):
- File rows with pending proposals show an indicator (e.g., a small pencil dot).
- Opening the file shows a Pending Edits panel above the comments panel with
  Accept / Reject / View Diff buttons.
- Accept → calls a new `accept_proposed_edit(id)` Tauri command that does
  `save_document` with the stored source + base_hash. Rejects with conflict
  if the disk has moved.
- Reject → marks the proposal `status = 'rejected'`, removes the synthetic
  comment, leaves `review_state` as the user sets it manually.

### 4b. `commit_edit`

Args: `{ path, base_hash, new_source }`. The direct-write path.

Auth gate: each session row has an `auto_commit` boolean (default false).
The agent panel exposes a toggle "Auto-commit edits from this agent" per
session. While off, `commit_edit` returns
`EDIT_REQUIRES_USER_APPROVAL` — agent must fall back to `propose_edit`.

When auto-commit is on: `commit_edit` calls `save_document` directly.

### Acceptance for Slice 4

- Scripted `propose_edit` creates a proposal; UI shows pending-edit indicator.
- User can Accept the proposal; file is written; review_state clears as
  expected.
- `commit_edit` is rejected with `EDIT_REQUIRES_USER_APPROVAL` until the
  user toggles auto-commit, then succeeds.

**Commit:** `feat(mcp): propose_edit + commit_edit with user approval gate`

---

## Slice 5 — Agent panel integration (~60 min)

### 5a. MCP-originated agent sessions in the panel

The existing agent panel (manual-add) gains automatic entries from active MCP
connections. Each MCP session shows:
- Persona badge (registry color)
- Agent name + project (`cpo · claude · agent-canvas`)
- A green dot indicator while connected
- Attached artifacts (from `attach_artifact`)
- Auto-commit toggle (default off; persisted in `agent_sessions.auto_commit`)
- "Disconnect" button (sends `notification/shutdown` to that session only)

Manual sessions (pre-MCP) coexist; `source = "manual"` vs `source = "mcp"`
distinguished by an icon.

### 5b. Send-to-Agent prefers MCP sessions

When a user invokes Send-to-Agent on an artifact, if the targeted persona has
an active MCP session, the message is INSERTED into `user_messages` (which
`get_user_messages` reads) AND the pasteboard handoff still copies the
formatted payload. The agent gets a real-time `notify_user`-style toast
on its side via a new server-pushed event channel (out of scope for v0.3 —
agent must poll `get_user_messages`).

### 5c. Configurable persona registry path passes through to MCP server

The persona registry used for `initialize` validation comes from the same
config the UI uses (`Persona registry path` setting). `reload_persona_registry`
invalidates the cached registry the MCP server uses too.

### Acceptance for Slice 5

- Launch agent-canvas-mcp from a test script with persona=cpo,
  project=agent-canvas. Agent panel shows the session live.
- Disconnect button cleanly terminates the session and removes the row.
- Send-to-Agent on a cpo-targeted artifact populates `user_messages`; a
  scripted `get_user_messages` retrieves it.

**Commit:** `feat(mcp): agent panel integration — live sessions, auto-commit toggle, send→user_messages`

---

## Slice 6 — Distribution + docs (~45 min)

### 6a. Install path

`agent-canvas-mcp` ships inside the app bundle at
`AgentCanvas.app/Contents/Resources/bin/agent-canvas-mcp`. A "Reveal MCP
binary" command-palette entry surfaces this path so users can paste it into
their `~/.claude.json` / Codex MCP config.

### 6b. Config templates

In `docs/mcp-clients.md`:
- Claude Desktop / Claude Code `mcpServers` entry template
- Codex MCP config template
- Cursor / Windsurf / other-tool templates if straightforward

Each template includes the persona / project hint via the `agentCanvas`
extension block, with examples for cpo / cto / cfo personas.

### 6c. Status surface

`status_check` command-palette entry shows:
- MCP socket status (listening / not bound)
- Connected sessions (count + personas)
- Recent errors (last 10 lines of MCP server log)
- Path to the shim binary

### Acceptance for Slice 6

- "Reveal MCP binary" returns a path that exists when the dev/release build
  is installed.
- A user can copy that path into Claude Code's MCP config and connect.
- `docs/mcp-clients.md` is readable and tested against one real config.

**Commit:** `feat(mcp): distribution — bundled shim binary, client config docs, status check`

---

## Slice 7 — Release v0.3.0 (~30 min)

### 7a. Acceptance criteria smoke (manual)

All criteria below — pass/fail logged to `status.md`.

### 7b. Documentation refresh

- `README.md`: add an MCP section.
- `BUILD-SPEC-v0.md`: strike "Live MCP server" from Out-of-Scope.
- `intent.md`: NO change unless the user explicitly approves an
  amendment (intent is sacred — MCP is implementation of the existing
  destination, not a destination shift).

### 7c. Version bump + tag

- `0.2.2 → 0.3.0` across tauri.conf.json, Cargo.toml, ui/package.json.
- `git tag v0.3.0` on `main`.

**Commit:** `chore(release): v0.3.0 — live MCP integration`

---

## Acceptance Criteria (all must pass)

1. AgentCanvas running, agent-canvas-mcp launched from stdin; `initialize`
   completes within 200ms with valid `serverInfo`.
2. AgentCanvas NOT running, agent-canvas-mcp returns `AGENTCANVAS_NOT_RUNNING`
   and exits cleanly.
3. `tools/list` returns exactly 10 entries with valid JSON Schema for each.
4. `list_artifacts` returns Inbox files; passing `folder: "project"` with the
   session's declared project returns that project's files.
5. `list_artifacts` with a path argument outside session scope returns
   `PATH_OUT_OF_SCOPE`.
6. `get_current_focus` returns the artifact currently displayed in the UI;
   returns `null` when no artifact is selected.
7. `get_user_messages` returns only rows where `persona = session.persona`.
8. `notify_user` shows a toast in the UI within 250ms; action button (if
   provided) opens the referenced artifact.
9. `attach_artifact` adds the artifact as a sub-item under the agent's
   session row in the agent panel.
10. `add_comment` appends a comment to the sidecar; the comments panel
    shows it after reload.
11. `propose_edit` creates a pending-edit indicator; user can Accept and
    file is written with new base_hash; reviewing the diff before Accept
    works.
12. `commit_edit` returns `EDIT_REQUIRES_USER_APPROVAL` while auto-commit is
    off; succeeds while auto-commit is on.
13. Multiple sessions (cpo · claude · agent-canvas) and (cto · codex ·
    agent-canvas) connect simultaneously; both appear in the agent panel;
    `notify_user` from one does NOT leak to the other.
14. Cmd+Q during active sessions sends `notification/shutdown`; sessions
    disconnect cleanly with the right exit status.
15. Stale socket file (from a hard crash) is unlinked on next bind without
    user intervention.
16. Unknown persona on `initialize` is accepted with `persona = "default"`
    in the panel; no connection rejection.
17. Schema migration: an existing v0.2.2 SQLite DB upgrades cleanly on first
    v0.3.0 launch (additive tables for `proposed_edits`, `agent_attachments`,
    `user_messages`).
18. All 26 acceptance criteria from v0.2.0 still pass.

---

## Out of Scope for v0.3 (deferred to v0.4+)

- Cross-machine attach_artifact sync.
- Encrypted-at-rest sidecar.
- Per-artifact agent visibility (trust ladders).
- Auto-launch AgentCanvas from the shim when the app isn't running.
- HTTP+SSE transport (the modern alternative to stdio).
- A user-facing "Allow agent X to access project Y" approval UI on first
  connect.
- A built-in MCP test harness in the UI (use external test scripts during
  v0.3 build).
- Streaming responses for `get_artifact` (current scope: full source in one
  response; large-file pagination is deferred).
- Per-tool rate limiting.

---

## Constraints

- One atomic commit per slice; conventional commits.
- Co-Authored-By: Claude (planner) + Codex (implementer).
- Direct push to `main` after each slice when its acceptance criteria pass.
- A12–A21 invariants are NOT negotiable.
- All write paths from MCP go through existing Rust commands (A20).
- Any new Tauri command added must be path-bounded (A12).
- No new dialogs without the focus-management pattern (Slice 6e of v0.2-finish).

---

## Sequencing Rules

- Slices land in order 1 → 7. No skipping ahead.
- After each slice commits and acceptance criteria pass, push to origin/main.
- If a slice surfaces a blocker (compile fails, test fails, design gap), pause
  and surface to owner. Do not patch over by skipping items.
- Codex runs each slice as a separate `codex exec --full-auto` invocation,
  with this spec as the read-required reference. Claude verifies path-by-path
  after Codex finishes each slice.

---

## Open Questions (must resolve before Slice 1)

1. **Auto-launch behavior** — D1 currently says "no auto-launch from shim."
   Reconsider? Pro: smoother first-time experience. Con: surprise app launches
   from a CLI tool feel wrong.
2. **`get_user_messages` polling** — agents poll for new messages. Is the
   right cadence documented (e.g., every 5s)? Or should we add a
   server-pushed event channel in v0.3 instead of deferring?
3. **Persona namespace collision** — when two MCP sessions declare the same
   persona (e.g., two `cpo` sessions), do they share `user_messages`? Or are
   messages keyed by session-id?
4. **Sidecar comment author when MCP-added** — store as `persona` (e.g.,
   "cpo") or `persona · agent` (e.g., "cpo · claude")? The latter is more
   informative but adds a special-case in comment rendering.
5. **Should the shim binary detect AgentCanvas updates?** If the app is
   updated mid-session, the protocol version may have moved; should the shim
   reject and prompt the agent to reconnect?
6. **`attach_artifact` and the existing pin** — does attach automatically
   pin? D3 currently says optional via `also_pin`. Default true or false?
7. **`commit_edit` audit trail** — should every commit_edit insert a row into
   a new `edit_audit` table for later review? Useful for "what did agents
   actually change" but adds storage. Defer to v0.4?

---

*Spec complete. Awaiting owner review before Slice 1 implementation begins.*
