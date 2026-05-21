# AgentCanvas User Guide

AgentCanvas is a workbench for reviewing artifacts that LLM agents produce. Instead of agents dumping HTML or Markdown into a terminal, they hand the file to AgentCanvas. You review, annotate, and send feedback back to the agent through the same channel — no copy-paste shuffling.

This guide covers: install · first launch · connecting an agent · the round-trip workflow · every UI surface · troubleshooting.

---

## 1. Install

### From source (the current path)

```sh
git clone <repo> && cd agent-canvas
cd ui && CI=true pnpm install
cd ..
./scripts/install-release.sh
```

`install-release.sh` builds the `.app`, copies it to `/Applications/AgentCanvas.app`, strips the macOS quarantine attribute, registers it with LaunchServices, and launches it. First build is slow (5–15 min cold cargo compile). Subsequent builds are 10–60 seconds.

Verify it's installed:

```sh
ls /Applications/AgentCanvas.app
open -a AgentCanvas
```

After install, open it from Spotlight (⌘Space → "AgentCanvas"), the Launchpad, or drag it to your Dock.

### Dev mode (for iteration)

```sh
./scripts/launch-dev.sh
```

Runs the debug binary against a vite dev server at `localhost:1420`. Faster rebuilds, hot reload. Don't run dev and release at the same time — they fight over the MCP socket.

---

## 2. First launch — what gets created

On first launch, AgentCanvas creates:

| Path | What |
|---|---|
| `~/iCloud/AgentCanvas/` | The canvas root. Resolves to `~/Library/Mobile Documents/com~apple~CloudDocs/AgentCanvas/`. Contains `inbox/`, `archive/`, and one folder per project. |
| `~/Library/Application Support/AgentCanvas/state.db` | SQLite database with file tracking, comments, agent sessions, attachments, queued user messages. |
| `~/Library/Application Support/AgentCanvas/mcp.sock` | Unix domain socket for MCP clients (Claude Code, Codex, Cursor). |

If iCloud Drive isn't enabled, AgentCanvas shows an error modal explaining what to do — it doesn't panic.

You'll see an empty canvas. The next step is to give it some files to track.

---

## 3. Tracking files

AgentCanvas has two file flavors:

- **Flavor 1 — Inside canvas root.** Files in `~/iCloud/AgentCanvas/` are discovered automatically by the recursive watcher. Drop a file in `inbox/` from Finder and it appears immediately.
- **Flavor 2 — Anywhere on disk.** Tracked by absolute path. Agents add these via `attach_artifact(path)` over MCP, or you can drag any file from Finder into the canvas window and it gets tracked in place (no copy).

Both flavors get the same comments, viewers, and Send-back behavior. The multi-path watcher in v0.3 covers Flavor 2 explicitly so push notifications fire when an agent (or anything else) writes to a tracked file outside the canvas root.

**File rows show:**
- `💬 N` badge if the file has open comments
- Persona color stripe if the file's frontmatter declares one
- Just-arrived pulse for ~3 s after the file appears

**Right-click a file row** for: Open, Pin, Send to Agent, Move to Project, Archive, Reveal in Finder, Copy Relative Path, Delete file from disk.

---

## 4. Viewers

| Format | Renderer | Editing |
|---|---|---|
| Markdown (`.md`) | ProseMirror rendered preview + CodeMirror source | Yes — source-preserving save with `base_hash` guard; rendered-edit available with source fallback |
| HTML (`.html`, `.htm`) | Sandboxed iframe with `srcdoc` + bootstrap | View only — agents author HTML through their file tools |
| PNG / JPG | `<img>` with dimensions strip | View only |
| PDF | Sandboxed iframe with `file://` href | View only |
| JSON | CodeMirror + collapsible tree | Source edit, source-preserving save |
| TXT / unknown text | CodeMirror plain | Source edit, source-preserving save |

Every viewer has a toolbar button: **"Add comment about this file"** → opens the file-level comment dialog.

### Interactive HTML

HTML rendered in the iframe is **interactive but contained**:

- Sandbox flags: `allow-scripts allow-forms allow-popups allow-downloads`
- `allow-same-origin` is **off** — no DOM access to the host
- `allow-modals` is **off** — `alert()` / `confirm()` are silently swallowed (no wedge)
- Inside the iframe a bootstrap script gives you:
  - selectionchange → host (so ⌘⇧M on a selection opens a comment)
  - console capture (iframe `console.log` lands in main `Console.app`)
  - `window.agentcanvas.sendBack({note, action_verb})` API agents can call from generated HTML
  - `window.agentcanvas.scrollToSnapshot(text)` for re-opening at a previous selection

Sibling assets (`./style.css`, `./chart.js`) load from the same directory via the Tauri asset protocol — `tauri-plugin-persisted-scope` keeps the scope persistent across launches.

---

## 5. Connecting an agent

You connect an agent **once per client**. The same socket serves any number of concurrent sessions.

### Step 1 — Install the MCP client

In AgentCanvas, press ⌘K (command palette) and run **one** of:

- **Install for Claude Code** → writes `~/.claude.json` `mcpServers.agent-canvas` entry
- **Install for Codex** → writes `~/.codex/config.toml` `[mcp_servers.agent-canvas]` block
- **Install for Cursor** → writes `~/.cursor/mcp.json` `mcpServers.agent-canvas` entry

All three are idempotent — replace the entry if it exists; preserve other entries in the config.

Each writes the absolute path to the `agent-canvas-mcp` shim binary. The shim auto-launches AgentCanvas if it isn't already running.

### Step 2 — Teach the agent how to use the canvas

In your project's CLAUDE.md / AGENTS.md / `.cursor/rules`, paste the contents of [`docs/claude-md-template.md`](claude-md-template.md). That snippet tells the agent:

- When to call `open_artifact(path)` — after writing an HTML or MD output the user should review
- What to do on `notifications/artifact_updated{by:"user"}` — re-read the file, treat the `note` as the next instruction
- How to call `add_comment` when annotating user work
- The shape of a typical agent → user → agent round trip

### Step 3 — Verify

Start a new agent session in any project. The shim auto-launches AgentCanvas. In AgentCanvas's agent panel (right side), you should see a row appear with a **green dot**, the agent's `persona·agent` chip, and the project name.

---

## 6. The round-trip workflow

```
1. Agent writes a file (its normal Write tool)
2. Agent: attach_artifact(path) + open_artifact(path) + optional notify_user
3. AgentCanvas window foregrounds; file appears in inbox with pulse
4. You review:
   - read rendered HTML / MD / PDF
   - select text → ⌘⇧M → inline comment
   - viewer toolbar "Add comment about this file" → file-level note
5. Right-click the file → "Send back to {persona}·{agent}"
   - choose action verb: Revise · Critique · Expand · Summarize · Respond-to · custom
   - optional free-text note
6. Agent receives notifications/artifact_updated { by:"user", note, action_verb }
7. Agent re-reads file + sidecar comments, rewrites, returns to step 2
```

**Action verbs** are short, structured intent the agent will respect. You can edit the verb templates per-project.

**Comments** are durable — they live in a sidecar JSON next to the file (`<file>.comments.json`). They survive moves, restarts, and re-attaches. Comment authorship is `persona·agent` for MCP-added comments; `local` for ones you add by hand.

---

## 7. The 9 MCP tools (agent-facing reference)

| Tool | Args | Returns | When agents use it |
|---|---|---|---|
| `list_artifacts` | `filter?` | `[{path, name, ...}]` | Browse canvas (inbox + project + attached + pinned by default) |
| `get_artifact` | `path` | `{bytes, mime, metadata}` | Read a file's bytes |
| `get_current_focus` | — | `{path} \| null` | "What is the user looking at right now?" |
| `get_comments` | `path` | `[{id, author, anchor, body, created_at}]` | Pick up review notes |
| `get_user_messages` | `since?` | `[{path, note, action_verb, created_at}]` | Drain queued Send-back notes |
| `open_artifact` | `path` | `{tracked, was_already_known}` | Foreground canvas + show file |
| `attach_artifact` | `path, also_pin?` | `{attached}` | Mark file as in-context for this session |
| `notify_user` | `severity, message, action?` | `{delivered}` | Show toast in canvas |
| `add_comment` | `path, anchor, body` | `{comment_id}` | Drop annotation as persona·agent |

**No write tools.** Agents create files with their own Write / Edit tools, not through AgentCanvas. This keeps the agent's full file context (versioning, sandboxing) intact.

**Push notifications** the agent receives via `notifications/subscribe`:

| Notification | Fired when |
|---|---|
| `notifications/artifact_updated` | A file the session attached changed (by user via Send-back OR by file-watcher) |
| `notifications/artifact_focused` | The user clicked into a file this session attached |
| `notifications/shutdown` | Disconnect button, ⌘Q, or socket shutdown |

---

## 8. UI surfaces

### Sidebar (left)
- **Inbox** — files with `in_inbox=true`
- **Projects** — folder per project; live count badges; rename / delete-if-empty inline
- **Archive** — soft-deleted files
- ⌘F to filter

### Content pane (middle)
- File list (scrollable, persona color stripes, comment badges)
- Selected file's viewer in a content pane below
- Floating annotation toolbar appears on text selection in Markdown rendered view

### Agent panel (right)
- Live MCP sessions: green dot, `persona·agent` chip, project, attached artifact list, **Disconnect** button
- Manual sessions: grey dot, **Remove** button
- "+ Add Agent" for pre-MCP manual workflows
- Sessions auto-disappear from the panel when they disconnect (no archived view in v0.3)

### Command palette (⌘K)
- Reload Persona Registry
- Install for Claude Code / Codex / Cursor
- Per-project quick switchers

### Keyboard shortcuts
| Key | Action |
|---|---|
| ⌘K | Command palette |
| ⌘F | Filter file list |
| ⌘⇧M | Add inline comment on selection |
| ⌘N | New file (in active project) |
| F2 | Rename selected file |
| ⌘Q | Quit (sends `notifications/shutdown` to every MCP session first) |
| Esc | Close any open modal / popover |

---

## 9. Three-way merge

If a file changes on disk while you have unsaved edits in the rendered editor, AgentCanvas opens a 3-column merge dialog (your draft / common ancestor / current disk). Per-block resolve. No last-write-wins anywhere.

The merge UI uses the sidecar's `base_snapshot` (recorded on every successful save) as the ancestor. If no `base_snapshot` exists, you get a Replace / Keep Both / Cancel conflict modal instead.

---

## 10. Persona registry

Persona identity drives badge color + `persona·agent` author strings in comments.

The registry lives at `~/code/_shared/pike-agents/plugins/` by default. If absent, AgentCanvas falls back to a built-in default table. The path is configurable. The registry has a built-in cache; ⌘K → **Reload Persona Registry** invalidates it (both for the UI badge map and the MCP-side `initialize` validator).

Unknown personas don't reject — they're tagged `default` for the session. You'll see a warning in the agent panel.

---

## 11. Troubleshooting

| Problem | Check |
|---|---|
| "AgentCanvas isn't responding" when agent calls a tool | Is the socket present? `ls ~/Library/Application\ Support/AgentCanvas/mcp.sock`. Is the app running? Stale socket from a hard crash unlinks itself on next launch. |
| Shim auto-launch failed | Run `open -a AgentCanvas` manually. Check Console.app for `agent-canvas-mcp` log. |
| `persona unknown` warning | Add the persona to `~/code/_shared/pike-agents/plugins/`, then ⌘K → Reload Persona Registry. Session continues with `default` until reload. |
| Comments not persisting on PNG / PDF | v0.3 fixed the binary-unsafe sidecar load. Verify the sidecar JSON exists next to the file: `ls <file>.comments.json`. |
| Send-back goes to clipboard instead of MCP session | No MCP session has called `attach_artifact(path)` on that file. Either the agent needs to attach it, or the route falls back to clipboard intentionally. |
| Two agent sessions show same `persona·agent` | They're different sessions with the same persona. Send-back routes to whichever attached the file most recently; if both attached, you'll get a picker. |
| Dev binary + release `.app` collide | They share the same socket and DB. Kill one before launching the other; `scripts/install-release.sh` does this automatically. |

### Resetting

| To reset… | Delete… |
|---|---|
| All comments | `~/iCloud/AgentCanvas/**/<file>.comments.json` |
| All tracking state (will rescan canvas root) | `~/Library/Application Support/AgentCanvas/state.db` (will be recreated) |
| MCP socket only (leaves DB alone) | `~/Library/Application Support/AgentCanvas/mcp.sock` (app rebinds on restart) |
| Persona cache | ⌘K → Reload Persona Registry (no delete needed) |

---

## 12. What's not in v0.3 (v0.4+ candidates)

- **Positional comments on PNG / PDF** — file-level only today. Point-and-pin coming.
- **Shape annotations** (circles, arrows, callouts overlay on any viewer).
- **Pending Reviews aggregate view** — per-file review state ships; cross-file aggregate panel doesn't.
- **Cross-machine sync of state.db**.
- **Notarization + signed binaries** — v0.3 ships ad-hoc/dev-signed.
- **iOS reader**.
- **Per-session permission scopes** — every session has full canvas read access; no per-file ACL.

For the full scope ledger see `BACKLOG.md` and `BUILD-SPEC-v0.md`.

---

## 13. Where to learn more

| For… | Read… |
|---|---|
| Why AgentCanvas exists | `intent.md` |
| Implementation plan (v0.3) | `docs/BUILD-SPEC-v0.3.md` |
| Per-slice change reports | `docs/active/codex-slice{1..7}-v0.3-report-*.md` |
| MCP client install / config shapes | `docs/mcp-clients.md` |
| Agent CLAUDE.md template | `docs/claude-md-template.md` |
| Decisions (why things are the way they are) | `decisions.md` |
| Visual tokens (authoritative) | `prototypes/visual-system.md` |
| Current state / open work | `status.md` + `BACKLOG.md` |
