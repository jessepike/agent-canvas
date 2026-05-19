# AgentCanvas — Backlog

Status: `todo` / `in-progress` / `blocked` / `done` / `cut`.

## v0.1.0 Build Slices

| Slice | Status | Item |
|---|---|---|
| 1 | done | Migration scaffolding: package rename, legacy docs, fresh React shell, context docs |
| 2 | done | iCloud substrate, SQLite state, inbox list view |
| 3 | todo | Markdown render, edit mode, atomic save with conflict banner |
| 4 | todo | Sandboxed HTML render and source toggle |
| 5 | todo | Recursive watcher, debounce, UI invalidation |
| 6 | todo | Persona registry from pike-agents with SQLite cache and fallback |
| 7 | todo | Pasteboard Send-to-Claude payload |
| 8 | todo | Manual agent panel backed by SQLite sessions |
| 9 | todo | Command palette with real keyboard wiring |
| 10 | todo | Project mode with three-column layout |
| 11 | todo | Keyboard polish, rescan-on-focus, smoke test |
| 12 | todo | README, Apache 2.0 license, release status, v0.1.0 tag |

## v0 Scope

- iCloud-backed artifact inbox at `~/iCloud/AgentCanvas/`.
- Markdown and HTML artifact viewing.
- Source-only editing with Vellum's atomic save and stat+hash guard.
- SQLite sidecar state at `~/Library/Application Support/AgentCanvas/state.db`.
- Persona registry from pike-agents frontmatter with built-in fallback.
- Pasteboard handoff to Claude/Codex.
- Manual agent session declaration panel.
- Command palette and keyboard-first navigation.

## Cut From v0

- Live MCP server or socket protocol.
- Comments and anchoring.
- Rendered ProseMirror editing.
- Three-way merge UI.
- Annotation toolbar.
- PNG, JSON, TXT viewer modes.
- Pending Reviews aggregate view.
- Cross-machine sync of `state.db`.
- iOS reader.
- Notarized/code-signed release.
- Search index.
- Trust boundaries or per-artifact agent visibility.

## Carry-Forward From Legacy Vellum

- Markdown block parser and format-preservation corpus.
- Same-directory tmpfile atomic save.
- Stat+hash optimistic concurrency guard.
- Tauri 2 Rust substrate and IPC patterns.
