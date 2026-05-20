# AgentCanvas

AgentCanvas is a local Mac workbench for reviewing, lightly source-editing, and round-tripping artifacts produced by LLM agents. It watches a plain-file iCloud folder, renders Markdown and HTML, preserves source bytes on save, and copies a formatted handoff payload for Claude/Codex sessions.

This repo was formerly Vellum. The old executable-block editor spec is preserved in `legacy/` only for parser, corpus, atomic-save, and Tauri carry-forward.

## Recent

v0.2.0 completes the v0 surface (pre-MCP):

- **Correctness:** every path-touching IPC command is canonically bound to `~/iCloud/AgentCanvas/`. Artifact identity is `(path, hash)` — same-content duplicates stay independent. Bootstrap shows an in-window error modal on iCloud failure instead of panicking. `last_read_at` is written on open.
- **Search & sidebar:** real `⌘F` search filters the visible list. Sidebar project rows show live artifact counts and support in-app Rename / Delete-if-empty. Command palette has "Reload Persona Registry" — no restart to pick up new personas.
- **Personas:** badge colors are read from the registry (`Persona.color`), and the file frontmatter (`persona:`, `author:`, `agent:`) is parsed and matched against the registry, with mtime+size caching.
- **Files:** F2 rename with conflict / multi-select with `⌘`-click and shift-click / multi-file Send with N-file clipboard payload / in-app conflict modal (Replace · Keep Both · Cancel) / drag-out to Finder.
- **Viewers:** PNG (with dimensions strip), JSON (CodeMirror + collapsible tree), TXT/unknown-text (plain CodeMirror), PDF (sandboxed iframe).
- **Editing:** ProseMirror rendered editing with source-preserving save (falls back to source view when the rendered round-trip is uncertain). Floating annotation toolbar (Bold / Italic / Strike / Code / Mark-for-Revision). Three-way merge dialog replaces the conflict-banner on external change.
- **Workflow:** inline comments with raw-source anchors persisted in the sidecar; per-artifact review state (unread / reviewed / needs-work / approved) shown as colored row dots; editable action-verb templates appended to Send payloads.
- **Polish:** every modal is focus-trapped with first-focus, Escape-close, and focus-restore. Command palette has per-project rows. Empty-state copy refreshed.

v0.1.1 tightened the first real-use loop (kept for reference):

- Send-to-Agent payload with relative AgentCanvas path, fenced source, optional note, and action verb.
- Send popover with Review/Revise/Expand/Critique/Summarize/Respond-to/custom choices.
- Agent session labels are dynamic, with per-project defaults and a picker shortcut.
- Finder files can be dropped into Inbox; Inbox rows can be dragged to Projects or Archive.
- File-row right-click menu for open, pin, file-to-project, archive, send, reveal, copy relative path, delete.
- A titlebar `+` opens a native file picker and copies selected files into Inbox.

## v0.1.0

- iCloud substrate: `~/iCloud/AgentCanvas/` backed by `~/Library/Mobile Documents/com~apple~CloudDocs/AgentCanvas/`
- Inbox, Projects, and Archive folders
- SQLite state at `~/Library/Application Support/AgentCanvas/state.db`
- Markdown rendered preview with source edit mode
- Sandboxed HTML iframe rendering with source view
- Atomic save with stat+hash optimistic concurrency guard
- Recursive watcher plus rescan on focus
- Persona badges from `~/code/_shared/pike-agents/plugins/` with built-in fallback
- Pasteboard Send-to-Claude handoff
- Manual agent session panel
- Cmd-K command palette
- Keyboard-first inbox controls

## Visual Target

The UI follows the checked-in prototypes:

- `prototypes/A-main-daily-driver.png`
- `prototypes/B-html-artifact.png`
- `prototypes/E-agent-panel.png`
- `prototypes/F-project-detail-3col.png`
- `prototypes/I-command-palette.png`

Design tokens live in `prototypes/visual-system.md`.

## Development

Per project policy, run package installs, builds, tests, and dev servers in the OrbStack dev VM:

```sh
orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum && pnpm --dir ui install'
orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum && pnpm --dir ui run build'
orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum && cargo test --workspace'
orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum && cargo run -p vellum-corpus'
```

## Release Notes

v0.2.0 ships ad-hoc/dev only. Notarization and signed binaries are deferred.
Live MCP server / socket protocol is the only remaining v0-scope deferral and is the v0.2-proper target.

## License

Apache License 2.0. See `LICENSE`.
