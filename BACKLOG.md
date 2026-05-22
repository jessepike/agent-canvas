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
| 6 | done | MCP coordination tools and Send-back routing |
| 7 | done | Agent panel MCP integration, one-click MCP install, CLAUDE.md template |
| 8 | done | Release v0.3.0 — version bumps, README + v0 spec refresh, user-guide.md, install-release.sh, paper+ink icon, tag |

## v0.3 Critical fixes

- done — Watcher coverage gap for Flavor 2. `watch::watch_vault(&canvas_root, ...)` only watched `~/Library/Mobile Documents/com~apple~CloudDocs/AgentCanvas/`, but Flavor 2 tracks files by absolute path anywhere on disk. Fixed by replacing startup with `watch::start(...)`, watching the canvas root recursively, syncing the union of tracked DB paths into parent-dir watches, and resyncing after track/untrack/move/archive/delete/pin/rename membership changes. Added deterministic watcher tests plus MCP notification dispatch coverage.

## v0.4 candidates

- todo — Positional comments on PNG and PDF (Word/GDocs-style region anchors). PNG: click-to-pin and drag-to-rectangle, new anchor `png_region { x_pct, y_pct, w_pct?, h_pct? }`, pins rendered as numbered overlay. PDF: swap `<object>` to `pdf.js` so we control the canvas, new anchor `pdf_region { page, x_pct, y_pct, w_pct?, h_pct? }`. Slice 3 added `file_level` as the floor; this slice adds the real model. Lighter version: PNG-region-only + `pdf_page` anchor (page number, no region), defer pdf.js to a later slice.
- done — Flaky vellum-core `watch_emits_event_on_modify` test on macOS host. Resolved by normalizing watch paths and adding a lightweight snapshot poll fallback on the existing 200ms watcher cadence.

## v0.4 follow-ups (captured during v0.4 build)

- todo — [mcp-lock] Move `window.emit` out of the db-lock scope in `add_comment` and `notify_user` (mcp/tools.rs). Same class as the fixed open/attach deadlock (commit 95261f6) but NOT a hang — they emit while the dispatcher holds `state.db`, yet never re-lock `db`. Gate them into `needs_post_lock_side_effects` and run the emit post-lock. Latent main-thread risk only.
- todo — [preview] Make the rendered Markdown view re-parse the live edit buffer (debounced) so preview reflects unsaved edits. Single-pane toggle only — NO side-by-side (owner decision: less clutter). Retire the "Rendered-view editing lands in v0.3" banner. Confirm whether the MD parser is callable client-side or only via Rust IPC (decides instant vs round-trip). Scheduled after default-opener.
- todo — [send] Allow Send-to-agent to target any *connected* MCP session, not only one already attached to the current file. Today `sendCurrentArtifact` falls back to clipboard unless `attachedAgentOptions.length > 0` (App.tsx:1259). Relax the gate / auto-attach on send so the user can originate a handoff to a live agent. Core to the agent loop.
- todo — [layout/slice8] Header toolbar collides at normal widths ("Inbox | Edit | Send back to… | Comments | Add comment about this file" overflow/clobber). Redesign: one primary action (Send) visible; Edit + Comments-toggle as icon buttons; move "Add comment about this file" into the Comments pane header as a `+`. Surfaced 2026-05-22 (screenshot).
- todo — [layout/slice8] Comments pane has no close control and stays pinned open even when empty ("No comments"). Add `×` in the COMMENTS header; Comments button toggles the pane; don't pin an empty pane. Surfaced 2026-05-22.
- todo — [sessions/ghosts] `agent_sessions` rows survive an unclean app shutdown (force-quit) with `disconnected_at` NULL, so they reappear as "live" — screenshot showed 5 duplicate `cpo·claude` ghosts. Socket-close cleanup (commit cffcae5) only covers graceful disconnects. Fix: mark ALL agent_sessions disconnected on startup (no MCP connection survives a restart). Surfaced 2026-05-22.
- todo — [shim/resilience] The `agent-canvas-mcp` shim does a one-shot UnixStream connect; when the desktop app restarts (every install-release.sh recreates the socket at ~/Library/Application Support/AgentCanvas/mcp.sock), the shim's connection dies and Claude Code surfaces `Failed to reconnect: ENOENT`, requiring a manual `/mcp` reconnect. Make the shim retry/reconnect with backoff when the socket drops/reappears, so agents survive an app restart. Surfaced 2026-05-22.
- **PRIORITY** todo — [identity] Claude Code CANNOT send `clientInfo.agentCanvas`, so EVERY Claude Code connection lands as `default / unknown / unknown-session` — confirmed live 2026-05-22 (not the edge case originally assumed). Two concurrent Claude agents collide on the shared `unknown-session` (shared attachments + user_messages + a junk "default·unknown" label). Fix in the SHIM (we control it): the `agent-canvas-mcp` shim should inject the `clientInfo.agentCanvas` block into the forwarded `initialize` — generate a per-process UUID `session_id` and read persona/agent/project from MCP-config `env`/`args` (e.g. AGENTCANVAS_PERSONA). The "Install for X" command should write those env vars. Backstop in the app: when the block is absent, mint a per-connection UUID instead of the literal `unknown-session`, and label from `clientInfo.name` ("claude-code") rather than "unknown".
- todo — [nav/ia] Header still cluttered AFTER Slice 8 (owner, 2026-05-22, second report). Slice 8 made controls compact but not fewer (~9: Rescan · New · + · breadcrumb · Edit · Send · 💬 · Save · ⊡). Needs an IA pass: fold Rescan/New into the sidebar, push low-frequency actions (Rescan, collapse-all) into a `⋯` overflow, keep only Send (primary) + contextual Edit/Save + 💬 in the bar. Distinct from the [layout/slice8] visual declutter (done).
- todo — [pin] Confirm whether `open_artifact`/send auto-pins the artifact — owner saw handoff-event-schema.md pinned (pinned=1, in_inbox=1) after an agent sent it and was unsure if they pinned it. If auto-pinned by the agent flow, that's unintended; pinning should be a user action only. Surfaced 2026-05-22.
- todo — [ui-types] Extract the `mode` union (`"inbox" | "drafts" | "project" | "archive" | "pinned"`) to a shared type alias. Minor.

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

- todo — [v0.3-watcher-spinoff] Notification de-duplication at dispatch. End-to-end smoke shows watcher fires 4× per single write event (fsevents reports MODIFIED + ATTR_CHANGED + variants). Currently each fires its own `notifications/artifact_updated` down the socket. Dedupe at the watcher → emit boundary (e.g., 100ms coalesce per (path, by)). Minor noise issue; not blocking.
