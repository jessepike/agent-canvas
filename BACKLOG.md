# AgentCanvas — Backlog

Status: `todo` / `in-progress` / `blocked` / `done` / `cut`.

## v0.3 Build Slices

| Slice | Status | Item |
|---|---|---|
| 1 | done | Sidecar comments on Markdown selections |
| 2 | done | Interactive HTML viewer, postMessage bridge, comments-on-HTML |
| 3 | done | File-level comments and grouped comments panel |
| 4 | done | MCP server skeleton, stdio shim, initialize, tools/list |
| 5 | done | MCP read tools and push notification channel |

## v0.3 Critical fixes

- todo — Watcher coverage gap for Flavor 2. `watch::watch_vault(&canvas_root, ...)` only watches `~/Library/Mobile Documents/com~apple~CloudDocs/AgentCanvas/`, but Flavor 2 tracks files by absolute path anywhere on disk. Files tracked outside canvas root never fire `mcp::emit_artifact_updated`, so MCP clients never receive `notifications/artifact_updated`. Fix: watch the parent directory of every tracked path (notify supports multi-path), refresh on track/untrack events. Without this, the agent loop (Send-back → agent reads update) doesn't function for the majority of tracked files. **Recommended before Slice 6 acceptance.**

## v0.4 candidates

- todo — Positional comments on PNG and PDF (Word/GDocs-style region anchors). PNG: click-to-pin and drag-to-rectangle, new anchor `png_region { x_pct, y_pct, w_pct?, h_pct? }`, pins rendered as numbered overlay. PDF: swap `<object>` to `pdf.js` so we control the canvas, new anchor `pdf_region { page, x_pct, y_pct, w_pct?, h_pct? }`. Slice 3 added `file_level` as the floor; this slice adds the real model. Lighter version: PNG-region-only + `pdf_page` anchor (page number, no region), defer pdf.js to a later slice.
- todo — Flaky vellum-core `watch_emits_event_on_modify` test on macOS host. File watcher timing under macOS notify; passes in Linux VM. Either add longer wait or skip the test on macos cfg.

## v0.3 Spinoffs

- todo — [v0.3-slice2-spinoff] Replace the `ts-rs` warning-prone `CommentAnchor` export with an explicit generated-type strategy or custom TS binding so `serde(skip_serializing_if)` stays warning-free while preserving legacy sidecar output.

## v0.1.0 Build Slices

| Slice | Status | Item |
|---|---|---|
| 1 | done | Migration scaffolding: package rename, legacy docs, fresh React shell, context docs |
| 2 | done | iCloud substrate, SQLite state, inbox list view |
| 3 | done | Markdown render, edit mode, atomic save with conflict banner |
| 4 | done | Sandboxed HTML render and source toggle |
| 5 | done | Recursive watcher, debounce, UI invalidation |
| 6 | done | Persona registry from pike-agents with SQLite cache and fallback |
| 7 | done | Pasteboard Send-to-Claude payload |
| 8 | done | Manual agent panel backed by SQLite sessions |
| 9 | done | Command palette with real keyboard wiring |
| 10 | done | Project mode with three-column layout |
| 11 | done | Keyboard polish, rescan-on-focus, smoke test |
| 12 | done | README, Apache 2.0 license, release status, v0.1.0 tag |

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
