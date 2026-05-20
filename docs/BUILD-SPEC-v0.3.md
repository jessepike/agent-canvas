# AgentCanvas v0.3 — Interactive Agent Workbench

**One-line goal:** make AgentCanvas the local interactive surface where agents put HTML / images / markdown they want you to review, and where you talk back richly without ever leaving for a terminal.

**Driving source:** [Anthropic, "Using Claude Code: the unreasonable effectiveness of HTML"](https://claude.com/blog/using-claude-code-the-unreasonable-effectiveness-of-html). Agent-produced HTML is the right output format for review / specs / dashboards / prototypes. Terminal text is not. AgentCanvas closes the loop the post describes: agent writes → user reviews + comments + interacts → user sends back → agent iterates.

**What this release adds:**
1. File-path tracking (no canvas folder; files live wherever the agent wrote them).
2. Inbox / Projects / Archive as **tags**, not directories.
3. Fully interactive HTML with safe sandbox.
4. Comments that work on HTML (currently broken) and on every viewable file type.
5. A live MCP server so agents can open files, get notified of user feedback, and read what the user said.

**Out of scope (explicit, deferred to v0.4+):**
- Shape annotations (circles, arrows, callouts as drawn overlays). Text comments only in v0.3.
- Agent file writes through canvas. Agents own writes via their own tools.
- Cross-machine sync.
- Per-artifact agent permission UX. You trust your own agents.

---

## Reads-required (every Codex slice)

1. This file
2. `intent.md`
3. `CLAUDE.md`
4. `BUILD-SPEC-v0.md`, `docs/BUILD-SPEC-v0.2-finish.md` — what's already built
5. `crates/agent-canvas-app/src/main.rs`, `crates/vellum-core/src/sidecar/mod.rs`
6. `ui/src/App.tsx`, `ui/src/components/SourceView.tsx`, `ui/src/ipc.ts`

---

## Architecture decisions (locked before any code)

| ID | Decision |
|----|----------|
| **D1** | **File model = absolute path.** No canvas folder. Files tracked in SQLite `files` table by absolute path. They live wherever the agent (or user) wrote them. |
| **D2** | **Inbox / Projects / Archive = tags, not directories.** New columns: `in_inbox BOOL`, `project_tag TEXT NULL`, `archived BOOL`. Sidebar reads from tags. No more `fs::rename` to move files between sections. |
| **D3** | **Remove ≠ Delete.** `×` button = untrack row from DB (file stays on disk). Explicit "Delete file from disk" available via context menu submenu, always confirmed regardless of the v0.2.2 "confirm before remove" toggle (the toggle now affects untrack only). |
| **D4** | **iCloud invariant deleted.** `path_within_canvas` removed. Path safety becomes "absolute, canonicalized, not a symlink loop, not a system path like /etc". User can open any file they have read access to. Default Inbox auto-population folder becomes `~/AgentCanvas/Inbox/` (local, not iCloud) for drag-in from Finder — but tracked files can live anywhere. |
| **D5** | **HTML viewer: srcdoc + sandbox.** Default render path: read file source via Tauri command, pass into `<iframe srcdoc>` with `sandbox="allow-scripts allow-forms allow-popups allow-downloads"`. **No `allow-modals`** (alert/confirm/prompt no-op silently). **No `allow-same-origin`** (iframe in opaque origin). Full JS / forms / buttons / fetch-to-public-APIs work. Wedging the parent is structurally impossible. |
| **D6** | **HTML sibling-asset mode.** If the HTML scan detects `<link rel="stylesheet" href="./...">` or `<script src="./...">` or `<img src="./...">`, render via `convertFileSrc(path)` instead of srcdoc, with `tauri-plugin-persisted-scope` (feature `protocol-asset`) so the file's parent directory becomes a persisted asset scope. Same sandbox flags. |
| **D7** | **postMessage is the HTML↔host channel.** Versioned protocol `agentcanvas-iframe/1`. Host injects a tiny bootstrap script into every iframe with: (a) selection-range bridge for comments, (b) `agentcanvas.sendBack(payload)` helper, (c) standardized event names. |
| **D8** | **MCP transport = unix domain socket + stdio shim.** Same as previous spec D1. Socket at `~/Library/Application Support/AgentCanvas/mcp.sock`. Separate `agent-canvas-mcp` binary forwards stdio JSON-RPC to socket. Agents launch the shim via standard MCP config. |
| **D9** | **Shim auto-launches AgentCanvas if not running.** If socket bind fails, shim invokes `open -a AgentCanvas.app`, waits up to 5s for socket to appear, then proceeds. Returns clean MCP error if launch fails. |
| **D10** | **Session identity = (persona, agent, project, session_id).** Declared on `initialize` via `clientInfo.agentCanvas` extension block. Two same-persona sessions (e.g., two cpo·claude windows) get distinct session-ids and distinct `user_messages` queues. |
| **D11** | **Comment author = `persona·agent`.** Stored verbatim in sidecar `Comment.author`. Rendered as a single chip in the comments panel. |
| **D12** | **Send-back = push notification down the socket.** Server emits a JSON-RPC notification `notifications/artifact_updated { path, by: "user", note?, action_verb? }` to the session(s) that have the file attached or last-opened. Poll fallback via `get_user_messages` for clients that don't subscribe. |
| **D13** | **Resolved/done is implicit.** No explicit `mark_resolved` tool. Send-back = "I'm done with this pass." Agent writes new content → file change → canvas re-highlights in Inbox → next round. User stops sending back = conversation done. |
| **D14** | **No agent write surface.** Killed from v0.3: `create_artifact`, `commit_edit`, `propose_edit`. Agents write files via their own tools. |
| **D15** | **v0.3 comment scope by file type.** Text-with-anchor on markdown (source view) and HTML (rendered view, via selection bridge). File-level comments (no anchor) on PNG / PDF / JSON / TXT. Point-and-pin / shape annotations = v0.4. |

---

## Invariants

A12 (path-bounding), A14 (registry-driven persona colors), A15 (visual-system tokens), A16 (single window chrome), A17 (in-app modals not window.prompt), A18-A21 from the previous draft — all carry forward except:

- **A1 (iCloud canvas root) — DELETED.** Replaced by D1+D4.
- **A12 (path-bounding) — RELAXED.** `path_within_canvas` becomes `path_safe_for_canvas` — checks absolute, canonicalized, not in `/etc /System /private /usr /var /Library/Application Support/{other apps}`, not a symlink loop. Anything else is fair game.

New for v0.3:

- **A22 — Iframe sandbox flags are fixed.** Every HTML iframe in the app uses exactly `sandbox="allow-scripts allow-forms allow-popups allow-downloads"`. No `allow-modals`. No `allow-same-origin`. Changes require a new ADR.
- **A23 — postMessage protocol versioned.** Host bootstrap script declares `protocol: "agentcanvas-iframe/1"`. Future breaking changes ship as `/2`. Agents writing custom HTML that posts back can target the version.
- **A24 — MCP write tools route through existing Rust commands.** Same as A20 from previous draft.

---

## The workflow loop

```
Agent in terminal writes file → ~/code/myrepo/report.html
   ↓
Agent → MCP: open_artifact("/abs/path/to/report.html")
   ↓
AgentCanvas:
   - auto-tracks file in DB (inbox tag = true)
   - foregrounds app via app.show() + dockShow + window.focus
   - selects + opens the file in content pane
   - "just-arrived" highlight in Inbox
   - notify_user-style toast: "{persona} sent a new file"
   ↓
User reviews:
   - Markdown: rendered preview default, source toggle, edit + save
   - HTML: interactive (buttons work, forms work, no script wedge possible)
   - Comments: select text + ⌘⇧M (works on md AND html now)
   - File-level note: a single textarea on PNG/PDF/JSON/TXT
   ↓
User clicks "Send back to {persona·agent}"
   ↓
MCP push: notifications/artifact_updated { path, note?, action_verb? }
   ↓
Agent receives. Re-reads file (get_artifact) + comments (get_comments).
Iterates in terminal.
   ↓
Agent writes new version (same path) or new file (different path)
   ↓
Canvas watcher sees change → re-highlight + bump in Inbox
   ↓
Loop until user stops sending back. Conversation implicitly done.
```

---

## MCP tool surface (8 tools — read-heavy, no agent writes)

| Tool | Args | Returns | Notes |
|------|------|---------|-------|
| `list_artifacts` | `{ filter?: { inbox?, project?, pinned?, archived? } }` | `Vec<ArtifactSummary>` | Tag-based filter. Session-scoped (sees Inbox + assigned project + Pinned by default). |
| `get_artifact` | `{ path }` | `{ source, base_hash, sidecar, kind }` | Returns full source + sidecar (comments + base_snapshot). |
| `get_current_focus` | `{}` | `{ path } \| null` | The file the user is looking at right now. |
| `get_comments` | `{ path, since?: epoch_seconds }` | `Vec<Comment>` | Comments on the file. `since` filters newer-than. |
| `get_user_messages` | `{ since?: epoch_seconds }` | `Vec<UserMessage>` | Messages from Send-Back targeted at this session. |
| `open_artifact` | `{ path }` | `{ tracked: bool, was_already_known: bool }` | **Core workflow trigger.** Foregrounds app, focuses pane, auto-tracks. |
| `notify_user` | `{ severity: "info"\|"warn"\|"error", message, action? }` | `{ delivered: bool }` | Toast in the UI. Action = optional `{ label, artifact_path }`. |
| `attach_artifact` | `{ path, also_pin?: false }` | `{ attached: bool }` | Marks file as "in-context" for this agent's session. `also_pin` defaults `false`. |
| `add_comment` | `{ path, anchor, body }` | `{ comment_id }` | Anchor: `{ block_id?, start_offset, end_offset }` or `{ kind: "file_level" }`. |

Server-pushed notifications (D12):

| Notification | Params | When |
|--------------|--------|------|
| `notifications/artifact_updated` | `{ path, by: "user"\|"watcher", note?, action_verb? }` | User clicked Send-back, OR file changed on disk (e.g., user-edit saved). |
| `notifications/artifact_focused` | `{ path }` | User clicked into a file. Optional subscription. |
| `notifications/shutdown` | `{}` | App quitting. |

---

## Slices

### Slice 1 — Flavor 2 data model (~90 min)

Drop the iCloud invariant. Move Inbox / Projects / Archive from filesystem layout to DB tags.

- DB migration: add `in_inbox BOOL`, `project_tag TEXT NULL`, `archived BOOL` to `files`. Backfill from current filesystem location for the existing iCloud DB.
- Replace `path_within_canvas` with `path_safe_for_canvas` (deny system paths, allow user paths).
- Rewrite `list_files` / `list_projects` / `list_archive` / `list_pinned` to query by tag, not by directory.
- Rewrite `move_file_to_project` / `move_file_to_archive` as DB updates only (no `fs::rename`).
- Replace `copy_paths_to_inbox` with `track_paths_in_inbox` — just registers DB rows pointing at the source paths. No file copying. Drag-from-Finder now tracks in place; if user wants a copy they can copy first.
- Default drag-in destination folder for "+" file picker stays a real folder (no need for it to be iCloud; default to `~/AgentCanvas/Inbox/` local, configurable).
- `delete_file` split: new `untrack_file` (default `×` action; removes DB row, keeps disk file) + existing `delete_file_from_disk` (rare, always-confirmed, explicit context-menu submenu).
- UI: sidebar reads tags. Drag-to-project / drag-to-archive update tags. Remove button (×) calls `untrack_file`.

**Commit:** `feat(v0.3-slice1): tag-based file model, drop iCloud canvas root, untrack vs delete`

### Slice 2 — Interactive HTML + selection bridge + HTML comments (~120 min)

The HTML viewer becomes a real interactive surface.

- HTML render path: read source via Tauri command, render in `<iframe srcdoc>` with the locked sandbox flags (A22).
- Sibling-asset detection: scan source for relative `./` href/src; if present, render via `convertFileSrc` and persist asset scope via `tauri-plugin-persisted-scope`.
- Inject host bootstrap script into iframe (via srcdoc prefix injection OR via `iframe.contentWindow` `postMessage` handshake — pick what works, document the choice). Bootstrap provides:
  - `agentcanvas.sendBack({ note?, action_verb? })` → host receives, triggers Send-back.
  - Selection bridge: bootstrap listens for `selectionchange`, posts `{ type: "selection", range: { start, end, text } }` to host.
  - Console bridge: bootstrap wraps `console.error` and posts to host for surfacing in UI error banner.
- Host listens for postMessages from iframe; routes selection events into the existing AnnotationToolbar mechanism so ⌘⇧M works on HTML the same way it works on markdown.
- Comment anchor for HTML: `{ kind: "html_selection", start_offset, end_offset, snapshot_text }` — offsets are character offsets into the rendered HTML's text content. Snapshot lets the comment show even if the underlying HTML changes.
- Add `tauri-plugin-persisted-scope` to Cargo.toml (feature `protocol-asset`).
- Configure `tauri.conf.json` `assetProtocol` with default allow `["$HOME/**"]` minus the system-deny list.

**Commit:** `feat(v0.3-slice2): interactive HTML viewer, postMessage bridge, comments-on-HTML`

### Slice 3 — File-level comments + unified comments UI (~45 min)

Comments work on every viewer.

- New anchor kind: `{ kind: "file_level" }`. Renders in comments panel without a "jump to selection" link.
- PNG / PDF / JSON / TXT viewers get a "Add comment about this file" button.
- Comments panel UI: groups by anchor kind (text selections at top, file-level at bottom).
- Sidecar schema gains nothing new (`CommentAnchor` already optional `block_id`).

**Commit:** `feat(v0.3-slice3): file-level comments on PNG/PDF/JSON/TXT`

### Slice 4 — MCP server skeleton (~90 min)

- New Cargo workspace member `crates/agent-canvas-mcp/` — stdio shim binary.
- Tauri-side: spawn tokio task at startup that binds `~/Library/Application Support/AgentCanvas/mcp.sock`, `unlink`s stale socket on bind.
- Frame format: newline-delimited JSON-RPC 2.0.
- `initialize` handshake: validates persona (graceful default for unknown), validates project, stores session row in `agent_sessions` table (`source = "mcp"`, session_id, persona, agent, project, connected_at).
- `tools/list` returns the 8-tool surface from above with valid JSON Schema.
- Shim auto-launch: on bind failure, invoke `open -a AgentCanvas.app`, poll socket for 5s, then attempt connect. Clean error if it fails.
- Graceful shutdown: on Cmd+Q, emit `notifications/shutdown`, unbind socket.

**Commit:** `feat(v0.3-slice4): MCP server skeleton, stdio shim, initialize, tools/list`

### Slice 5 — MCP read tools + push channel (~75 min)

- Implement `list_artifacts`, `get_artifact`, `get_current_focus`, `get_comments`, `get_user_messages`.
- New `user_messages` table; populated when user clicks Send-back.
- New `app_focus` field in `AppState`; UI updates it on every artifact selection.
- Server-pushed notification channel: when an event happens on the host (file change, user send-back, focus change), enqueue notifications to subscribed sessions.
- Session subscription model: clients opt in via `notifications/subscribe { events: ["artifact_updated", "artifact_focused"] }` after initialize. Default subscriptions: `artifact_updated`. Optional: `artifact_focused`.

**Commit:** `feat(v0.3-slice5): MCP read tools + push notification channel`

### Slice 6 — MCP coordination tools + Send-back routing (~90 min)

- Implement `open_artifact`, `notify_user`, `attach_artifact`, `add_comment`.
- `open_artifact` triggers: app foreground via `app.show()` + `Window::set_focus()` + dock activate, content-pane focus on file, auto-track if unknown (tag inbox=true), just-arrived highlight.
- Send-back UI rewrite: button label adapts ("Send back to {persona·agent}" when an MCP session attached the file; "Send to Agent" + clipboard fallback otherwise).
- When user clicks Send-back: insert row into `user_messages`, emit `notifications/artifact_updated` to attached session(s).
- Multi-session targeting: if >1 session attached the file, dropdown picker appears with default = the session that last opened it.

**Commit:** `feat(v0.3-slice6): open_artifact, notify_user, attach_artifact, add_comment, send-back push`

### Slice 7 — Agent panel + distribution + docs (~75 min)

- Agent panel: live MCP sessions appear as rows with persona badge, agent name, project, green-dot-connected indicator, attached artifacts as sub-items, Disconnect button.
- Manual-add sessions (pre-MCP) coexist with `source = "manual"`.
- Persona registry reload (`reload_persona_registry`) invalidates MCP-side cache too.
- "Install for Claude Code" / "Install for Codex" / "Install for Cursor" command-palette entries that write the MCP shim path into the right config file (`~/.claude.json`, etc.) automatically.
- `docs/mcp-clients.md` with manual install instructions + the `clientInfo.agentCanvas` extension block usage.
- `docs/claude-md-template.md` — a snippet users paste into their project CLAUDE.md telling agents how to use AgentCanvas (when to call `open_artifact`, what to do on `artifact_updated`).

**Commit:** `feat(v0.3-slice7): agent panel integration, one-click MCP install, CLAUDE.md template`

### Slice 8 — Release v0.3.0 (~30 min)

- Smoke through all acceptance criteria.
- Visual-system audit (still 0 raw-hex).
- README refresh: v0.3 capability summary.
- BUILD-SPEC-v0.md "Out of Scope": strike everything now shipped; keep v0.4 deferrals.
- Version 0.2.2 → 0.3.0 across Cargo.toml, tauri.conf.json, ui/package.json.
- Tag `v0.3.0`.

**Commit:** `chore(release): v0.3.0 — interactive agent workbench`

---

## Acceptance criteria (all must pass)

1. iCloud folder removed from app assumptions; app opens with no iCloud Drive present.
2. Drag a file from Finder into AgentCanvas → file row appears in Inbox tagged in_inbox=true, file stays at its original Finder location, no copies made.
3. Drag the file from Inbox to a Project in the sidebar → DB updates (project_tag set, in_inbox cleared), file on disk does not move.
4. Right-click → "Delete file from disk" → confirmation dialog → file gone from disk + canvas.
5. Click × on a tracked file → row disappears from canvas, file still on disk at its original path.
6. AgentCanvas not running, agent-canvas-mcp invoked → `open -a AgentCanvas.app` fires, app launches, socket binds, `initialize` succeeds within 5s.
7. AgentCanvas running, agent calls `open_artifact("/abs/path/foo.html")` → app comes to foreground, file appears in Inbox highlighted, content pane shows foo.html rendered interactively.
8. Interactive HTML test: agent-produced HTML with a `<button onclick="alert('hi')">` and a `<button onclick="window.parent.postMessage({type:'send-back',...},'*')">`. Alert is silently swallowed (no wedge). Send-back postMessage triggers the host Send-back flow.
9. Comments on HTML: select text inside rendered HTML, hit ⌘⇧M, comment dialog opens, save → comment persists in sidecar, reload → comment still there with selection highlighted.
10. File-level comment: open a PNG, click "Add comment about this file", save → comment in sidecar with `anchor.kind = "file_level"`.
11. Send-back round-trip: agent opens a file, user adds comment, user clicks "Send back to cpo·claude" with note "please revise", agent receives `notifications/artifact_updated` push within 250ms with `{ path, note: "please revise", action_verb? }`.
12. Same path, second write: agent writes v2 to the same path → file watcher fires → row re-highlights in Inbox → existing comments persist.
13. Two same-persona sessions (two cpo·claude windows) attached to different files → Send-back on one does NOT push to the other.
14. Unknown persona on initialize → session accepted with `persona = "default"` tag.
15. `get_current_focus` returns the file the user is looking at; returns `null` when nothing selected.
16. `get_user_messages` returns only messages for the calling session_id.
17. Cmd+Q while sessions are connected → `notifications/shutdown` to all, socket unbinds, no zombie processes.
18. Stale socket from a hard crash → next launch unlinks and rebinds cleanly.
19. `tauri-plugin-persisted-scope` lets HTML with sibling assets (`./style.css`, `./chart.js`) render with the assets loaded.
20. Comment author rendered as `cpo·claude` chip in comments panel for MCP-added comments.
21. All v0.2.x acceptance criteria still pass.

---

## Out of scope for v0.3 (v0.4+ targets)

- Shape annotations (circles, arrows, callouts overlay on any viewer).
- Comment anchoring to coords on PNG / PDF (point-and-pin).
- Cross-machine sync of canvas state.
- Encrypted-at-rest sidecar.
- Per-artifact agent permission UX.
- HTTP+SSE MCP transport.
- Auto-launching agents from canvas.
- Edit audit trail (`edit_audit` table) — irrelevant because v0.3 has no agent edit surface.
- Comment threading / @mentions.
- Notification routing to a phone / push to non-MCP channels.

---

## Open questions to resolve before Slice 1

1. **Default Inbox folder location for drag-in.** `~/AgentCanvas/Inbox/`? `~/Documents/AgentCanvas/Inbox/`? Configurable from first run? Recommendation: `~/AgentCanvas/Inbox/`, configurable.
2. **What happens to the existing iCloud folder on upgrade?** Migrate references in DB, leave files in place. Existing iCloud-tracked files keep working — their paths just point at `~/Library/Mobile Documents/com~apple~CloudDocs/AgentCanvas/...`. Recommendation: just leave alone; the path-tracking model means existing rows still work.
3. **MCP install command for non-Claude tools.** Codex MCP config path? Cursor's? Worth a quick check before Slice 7.
4. **Detecting "interactive HTML" vs "static HTML report".** Spec defaults both to interactive (srcdoc + sandbox). Recommendation: no distinction; everything's interactive, sandbox makes it safe.

---

## Constraints

- One atomic commit per slice; conventional commits with co-author Claude + Codex.
- Direct push to main after acceptance criteria pass.
- A12 (now A12-relaxed), A14-A24 invariants are not negotiable.
- All NEW Tauri commands must take a path go through `path_safe_for_canvas`.
- All NEW dialogs use the focus-management pattern from v0.2-finish Slice 6e.
- Visual system audit (0 raw-hex outside `:root`) must hold through every slice.

---

## Sequencing

Slices 1 → 8 in order. Slice 1 (data model shift) is the foundation — nothing else builds cleanly without it. Slice 2 (interactive HTML) is independent of MCP and can ship usefully even before MCP lands. MCP slices (4-7) build on top.

If any slice surfaces a blocker, pause and surface to Jesse. No silent scope drops.

---

*Spec ready for owner review. Awaiting go/no-go on slicing before Codex spins.*
