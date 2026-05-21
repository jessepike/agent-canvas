# AgentCanvas

AgentCanvas is a local Mac workbench for reviewing, lightly source-editing, and round-tripping artifacts produced by LLM agents. Agents call it through MCP to show the user HTML / Markdown / image outputs; the user reviews, comments, and pushes feedback back through the same channel — no clipboard handoff required.

The canvas lives at `~/iCloud/AgentCanvas/`. Files stay plain bytes; saves are atomic and `base_hash`-guarded; HTML renders in a locked sandbox; persona identity (`persona·agent`) follows every comment and notification.

This repo was formerly Vellum. The old executable-block editor spec is preserved in `legacy/` only for parser, corpus, atomic-save, and Tauri carry-forward.

## v0.3.0 — Interactive Agent Workbench (2026-05-21)

v0.3 connects AgentCanvas to live agents via MCP. The full agent-loop ships:

- **MCP server + stdio shim** — Unix socket at `~/Library/Application Support/AgentCanvas/mcp.sock` with auto-launch (`agent-canvas-mcp` shim wakes AgentCanvas if it isn't running). JSON-RPC 2.0 newline-delimited frames. Session identity `(persona, agent, project, session_id)` via `clientInfo.agentCanvas` on `initialize`.
- **9-tool MCP surface** — `list_artifacts` / `get_artifact` / `get_current_focus` / `get_comments` / `get_user_messages` (read) · `open_artifact` / `attach_artifact` / `notify_user` / `add_comment` (coordinate). No agent-write tools beyond `add_comment` — agents author files with their own file tools.
- **Push channel** — agents subscribe via `notifications/subscribe` and receive `notifications/artifact_updated` / `notifications/artifact_focused` / `notifications/shutdown` server-pushed events.
- **Send-back routing** — clicking "Send back to {persona}·{agent}" inserts a durable `user_messages` row and pushes a notification down the MCP socket. Multi-session picker. Falls back to the v0.2 clipboard handoff when no MCP session is attached.
- **Interactive HTML viewer** — sandboxed iframe with `allow-scripts allow-forms allow-popups allow-downloads` (no `allow-same-origin`, no `allow-modals` — invariant A22). selectionchange bridge, console capture, in-iframe ⌘⇧M to add a comment, `window.agentcanvas.sendBack()` API.
- **File-level comments on every viewer** — Markdown, HTML, PNG, PDF, JSON, TXT all accept `{kind:"file_level"}` anchors. HTML selections add `{kind:"html_selection", start_offset, end_offset, snapshot_text}` with scroll-to-snapshot highlight on reopen.
- **Agent panel** — live MCP sessions + manual sessions in one list with `persona·agent` chip, green dot when connected, attached artifacts as sub-items, Disconnect button.
- **One-click client install** — command palette entries for **Install for Claude Code**, **Install for Codex**, **Install for Cursor** — write the MCP shim path to `~/.claude.json`, `~/.codex/config.toml`, `~/.cursor/mcp.json` idempotently, preserving other entries.
- **Multi-path watcher** — Flavor 2 tracking (`add_path` / `remove_path` / `set_paths`) so files tracked outside `~/iCloud/AgentCanvas/` still drive the push channel.
- **Comment count surfacing** — sidebar badge `💬 N` per file row + viewer-toolbar "Comments (N) — add another" label.

See `docs/user-guide.md` for the full agent-loop walkthrough, and `docs/mcp-clients.md` for client install details. The canonical implementation plan is `docs/BUILD-SPEC-v0.3.md`.

## v0.2.0 — Core Workbench (2026-05-20)

Kept for reference:

- iCloud canvas root + atomic save with `base_hash` optimistic concurrency
- Inbox / Projects / Archive folders, SQLite state at `~/Library/Application Support/AgentCanvas/state.db`
- Markdown / HTML / PNG / PDF / JSON / TXT viewers
- ProseMirror rendered editing with source-preserving save + source fallback
- Annotation toolbar (Bold / Italic / Strike / Code / Mark-for-Revision)
- Inline comments with raw-source-offset anchors
- Three-way merge dialog on external change
- Per-artifact review state (unread / reviewed / needs-work / approved)
- Filename search (⌘F), command palette (⌘K), F2 rename, multi-select with ⌘-click + shift-click
- In-app conflict modal (Replace / Keep Both / Cancel), drag-out to Finder, drag-in from Finder
- Persona badges from `~/code/_shared/pike-agents/plugins/` with built-in fallback

## Visual Target

UI follows the checked-in prototypes:

- `prototypes/A-main-daily-driver.png`
- `prototypes/B-html-artifact.png`
- `prototypes/E-agent-panel.png`
- `prototypes/F-project-detail-3col.png`
- `prototypes/I-command-palette.png`

Design tokens live in `prototypes/visual-system.md` — authoritative; no raw hex in `ui/src/`.

## Installing

For day-to-day use, build the release `.app` and install to `/Applications`:

```sh
./scripts/install-release.sh
```

This installs `@tauri-apps/cli` if missing, builds the bundle, copies it to `/Applications/AgentCanvas.app`, strips the quarantine xattr, registers with LaunchServices, and launches. After install, ⌘Space → "AgentCanvas" opens it from Spotlight.

## Connecting an Agent

1. Open AgentCanvas, hit ⌘K, run **Install for Claude Code** (or Codex / Cursor).
2. Paste `docs/claude-md-template.md` into your project's `CLAUDE.md` / `AGENTS.md` / `.cursor/rules`.
3. Start a new agent session in any project. The shim auto-launches AgentCanvas if needed.

See `docs/user-guide.md` for the round-trip walkthrough.

## Development

```sh
# Dev mode (vite + debug binary, no LaunchServices, no Gatekeeper):
./scripts/launch-dev.sh

# Release build + install:
./scripts/install-release.sh

# Tests (OrbStack dev VM):
orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum && cargo test --workspace'
```

Build details and constraints are in `CLAUDE.md` (project context) and `docs/BUILD-SPEC-v0.3.md` (implementation plan).

## Release Notes

v0.3.0 ships ad-hoc/dev-signed only. Notarization and signed binaries are deferred to v0.4+. The build is verified working as a standalone `.app` on macOS 14+ arm64.

## License

Apache License 2.0. See `LICENSE`.
