---
project: agent-canvas
updated: 2026-05-21
stage: v0.3 watcher critical fix implemented
---

# AgentCanvas — Status

## v0.3 Watcher Critical Fix Summary (2026-05-21)

The Slice 6-blocking Flavor 2 watcher coverage gap is fixed.

- Replaced the app's single-root watcher startup with `watch::start(...)` plus `watch_recursive(&canvas_root)`.
- Added multi-path tracked-file watching in `vellum-core`: explicit files are tracked by normalized path, their parent directories are watched non-recursively with ref-counted unwatch, and the canvas root remains recursively watched for ad-hoc new files.
- Expanded watcher extension coverage to Markdown, HTML, PNG, JPG/JPEG, PDF, JSON, and TXT.
- Startup now syncs watcher paths from `files` rows where `in_inbox=1 OR project_tag IS NOT NULL OR archived=1 OR pinned=1`.
- Watcher resync now runs after track, untrack, project move, archive move, delete-from-disk, archive, pin toggle, and rename.
- Added deterministic watcher tests and an MCP notification dispatch test proving a watcher change can emit `notifications/artifact_updated` with `by="watcher"`.
- Wrote the implementation report to `docs/active/codex-watcher-v0.3-report-2026-05-21.md`.

Verification:

- `cd crates/vellum-core && cargo test 2>&1 | tail -20` passes.
- `cd crates/agent-canvas-app && cargo check -q` passes with the pre-existing ts-rs sidecar warning.
- `cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -25` passes: 27 tests.
- `cd crates/agent-canvas-mcp && cargo build` passes.
- `cd ui && ./node_modules/.bin/tsc --noEmit` passes.
- `cd ui && ./node_modules/.bin/vite build 2>&1 | tail -3` passes with the known chunk-size warning.

## v0.3 Slice 5 Summary (2026-05-21)

Slice 5 of `docs/BUILD-SPEC-v0.3.md` was implemented.

- Implemented real MCP read tools: `list_artifacts`, `get_artifact`, `get_current_focus`, `get_comments`, and `get_user_messages`.
- Added idempotent `user_messages` migration and session-scoped read filtering.
- Added per-session notification subscriptions with default `artifact_updated = true` and opt-in `artifact_focused`.
- Added server-pushed `notifications/artifact_updated`, `notifications/artifact_focused`, and shutdown delivery through the socket writer path.
- Wired focus updates from the UI through `set_current_focus`.
- Wired watcher/save paths to emit `artifact_updated` for tracked files; added a Tauri test command for manual Slice 6 send-back notification testing.
- Wrote the implementation report to `docs/active/codex-slice5-v0.3-report-2026-05-21.md`.

Verification:

- `cd crates/agent-canvas-app && cargo check -q` passes with the pre-existing ts-rs sidecar warning.
- `cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -25` passes: 26 tests.
- `cd crates/agent-canvas-mcp && cargo build` passes.
- `cd ui && ./node_modules/.bin/tsc --noEmit` passes.
- `cd ui && ./node_modules/.bin/vite build 2>&1 | tail -3` passes with the known chunk-size warning.
- `grep -rn 'allow-same-origin\|allow-modals' ui/src/ | grep -v '\.test\.' | wc -l` returns `0`.
- CLI initialize through the shim returned the expected JSON-RPC initialize response against the existing socket; full watcher-to-stdout smoke should be rerun with the freshly built GUI app running.

## v0.3 Slice 4 Summary (2026-05-20)

Slice 4 of `docs/BUILD-SPEC-v0.3.md` was implemented.

- Added the `agent-canvas-mcp` stdio shim binary.
- Added a Tauri-side Unix-domain socket MCP skeleton at `~/Library/Application Support/AgentCanvas/mcp.sock`.
- The app unlinks stale socket files before bind and removes the socket on graceful close.
- Implemented newline-delimited JSON-RPC handling for `initialize`, `notifications/initialized`, `ping`, `tools/list`, and `tools/call`.
- Added the 9-tool schema list from the v0.3 spec, including `add_comment`.
- Added stub JSON-RPC `-32601` errors for unimplemented tools; `list_artifacts` has a minimal DB-backed proof-of-life implementation.
- Added MCP `agent_sessions` table migration, with the previous manual agent-panel table migrated to `manual_agent_sessions`.
- Wrote the implementation report to `docs/active/codex-slice4-v0.3-report-2026-05-20.md`.

Verification:

- `cd crates/agent-canvas-mcp && cargo build` passes in OrbStack dev VM.
- `cd crates/agent-canvas-app && cargo build` passes in OrbStack dev VM.
- `cargo test --bin agent-canvas-app` passes in OrbStack dev VM: 16 tests.
- `cd ui && ./node_modules/.bin/tsc --noEmit` passes in OrbStack dev VM.
- `cd ui && ./node_modules/.bin/vite build` passes on host with the known large-chunk warning.
- VM Vite build is blocked by the existing Rollup optional dependency state in `ui/node_modules` (`@rollup/rollup-linux-arm64-gnu` missing).

## v0.3 Slice 1 Summary (2026-05-20)

Slice 1 of `docs/BUILD-SPEC-v0.3.md` was implemented.

- Added the Flavor 2 tag model to `files`: `in_inbox`, `project_tag`, and idempotent `archived` migration guard.
- Added legacy iCloud-path backfill from `Inbox/`, `Projects/{project}/`, and `Archive/` into DB tags.
- Replaced directory-scanned Inbox, Projects, Archive, and Pinned lists with tag-backed DB queries.
- Replaced drag/file-picker copy semantics with in-place tracking via `track_paths_in_inbox`.
- Reworked project/archive moves as DB tag updates only; disk paths do not change.
- Added `untrack_file` for the default row × removal path and `delete_file_from_disk` for explicit destructive deletion.
- Rewired UI removal, context menu, and IPC wrappers for untrack vs delete-from-disk.
- Wrote the implementation report to `docs/active/codex-slice1-v0.3-report-2026-05-20.md`.

Verification:

- `cd crates/agent-canvas-app && cargo check -q` passes.
- `cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -10` passes: 8 tests.
- `cd ui && ./node_modules/.bin/tsc --noEmit` passes.
- `cd ui && ./node_modules/.bin/vite build 2>&1 | tail -5` passes with the known large-chunk warning.
- CSS raw-hex regression prints `0`.

## v0.2.0 Release (2026-05-20)

All seven slices of `docs/BUILD-SPEC-v0.2-finish.md` are landed. Version bumped 0.1.1 → 0.2.0 across `ui/package.json`, `crates/agent-canvas-app/Cargo.toml`, `crates/agent-canvas-app/tauri.conf.json`. Tag `v0.2.0` created on `main`.

**Slice 7 — Release work performed:**

- **7a Smoke test (static):** Acceptance criteria 1-26 from `BUILD-SPEC-v0.2-finish.md` reviewed by code-path inspection. Live behavioral smoke is the user's pass (CLAUDE.md "you test then I test" rule).
- **7b Visual-system audit:** `grep -nE "#[0-9a-fA-F]{3,8}"` against `ui/src/styles.css` outside `:root` returns 0 raw-hex matches. All colors token-derived. A15 clean.
- **7b A17 audit:** `grep -rn "window\.prompt\|window\.confirm" ui/src/` returns 0 matches. The residual `window.confirm` on file-delete (Slice 4d carry-over) was replaced with a new `<ConfirmDialog>` (focus-trapped, Esc cancels, Enter confirms, destructive-button styling). A17 clean.
- **7c Docs refresh:** `README.md` updated with v0.2.0 capability summary. `BUILD-SPEC-v0.md` "Out of Scope for v0" section updated — only **Live MCP server** remains as a v0-scope deferral; everything else (comments, rendered editing, 3-way merge, annotation toolbar, viewers, search) is shipped.
- **7d Version bump + tag:** version strings synchronized; tag pushed.

**Verification (host):**

- `./node_modules/.bin/tsc --noEmit` — passes
- `./node_modules/.bin/vite build` — passes (with known large-chunk warning at ~1.2 MB; acceptable for v0.2; code-splitting deferred)
- `cargo check -q` / `cargo test --bin agent-canvas-app` (per Codex Slice 6 report) — 6 pass, 0 fail

**Out of scope for v0.2.0 (now v0.2-proper / v0.3 targets):**

- Live MCP server / socket protocol (the v0.2-proper deferred work)
- Pending Reviews aggregate view
- Cross-machine sync of state.db
- iOS reader
- Notarization / code-signing
- Trust boundaries / per-artifact agent visibility

**Known residuals (non-blocking, tracked for v0.3):**

- `pnpm build` is wedged by pnpm v11 ignored-builds approval (esbuild 0.25.12); workaround is `./node_modules/.bin/vite build` directly.
- Vite bundle is one 1.2 MB chunk. Code-splitting is a v0.3 polish item.

## v0.2-finish Slice 6 Summary

Slice 6 of `docs/BUILD-SPEC-v0.2-finish.md` was implemented on 2026-05-20.

- Added optional sidecar `comments` with raw-source-offset anchors, comment creation dialog, and collapsible comments panel.
- Added `files.review_state` additive migration, list-row review dots, manual "Mark as..." menu actions, first-open review transition, and Revise/Critique needs-work transition.
- Added persisted action templates in the SQLite `settings` table plus an action-template editor dialog.
- Added project-specific command-palette rows: `Open: {projectName}`.
- Added shared focus trapping / focus restoration for dialogs and the command palette.
- Improved empty-state copy across inbox, pinned, archive, and project views.
- Wrote the implementation report to `docs/active/codex-slice6-report-2026-05-20.md`.

Verification:

- `CI=true pnpm install --no-frozen-lockfile 2>/dev/null` installed packages but exited with pnpm 11 `ERR_PNPM_IGNORED_BUILDS` for `esbuild@0.25.12`.
- `pnpm build` was blocked by the same pnpm ignored-builds guard.
- `./node_modules/.bin/tsc --noEmit` passes.
- `./node_modules/.bin/vite build` passes with the known large-chunk warning.
- `cd crates/agent-canvas-app && cargo check -q` passes.
- `cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -5` passes: 6 tests.

No commit was made; orchestrator commits from host.

## v0.2-finish Slice 5a-d Summary

Viewer-only Slice 5a-d items from `docs/BUILD-SPEC-v0.2-finish.md` were implemented on 2026-05-20.

- Added path-bounded `read_binary_artifact` for PNG/PDF data URLs.
- Added PNG image viewer, JSON source/tree viewer, TXT plaintext viewer, and PDF iframe viewer.
- Added JSON CodeMirror language support through `@codemirror/lang-json`.
- Updated supported artifact detection for Markdown, HTML, PNG, JSON, TXT, and PDF.
- Wrote the implementation report to `docs/active/codex-slice5a-report-2026-05-20.md`.

Verification:

- `orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum/ui && pnpm install --no-frozen-lockfile'` passes.
- `orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum/ui && pnpm build'` passes. Vite reports the known large-chunk warning.
- `orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum/crates/agent-canvas-app && cargo check'` passes.
- `orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/vellum && cargo test --bin agent-canvas-app 2>&1 | tail -5'` passes: 6 tests.

No commit was made; orchestrator commits from host.

## v0.2-finish Slice 4 Extension Summary

Deferred Slice 4 items from `docs/active/slice4-partial-report-2026-05-19.md` were implemented on 2026-05-19.

- Added multi-file selection across inbox, project, archive, and pinned visible file lists.
- Added `MultiSelectPlaceholder` with bulk send, archive, and clear actions.
- Wired multi-file send UI to the existing `sendMultiToClipboard()` IPC path.
- Replaced the default-agent `window.prompt` with `AgentPickerDialog`.
- Added `Export to...` context-menu fallback using the native save dialog and new `export_file_to` command.
- Wrote the implementation report to `docs/active/codex-slice4ext-report-2026-05-19.md`.

Verification:

- `orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/agent-canvas/ui && pnpm build'` passes. Vite reports the known large-chunk warning.
- `orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/agent-canvas/crates/agent-canvas-app && cargo check'` passes.
- `orb run -m dev bash -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/agent-canvas && cargo test --bin agent-canvas-app 2>&1 | tail -5'` passes: 6 tests.

No commit was made; orchestrator commits from host.

## v0.2-finish Slice 3 Summary

Slice 3 of `docs/BUILD-SPEC-v0.2-finish.md` was implemented on 2026-05-19.

- Removed hard-coded non-built-in persona badge color classes and moved file/session persona badge colors to inline registry-derived `Persona.color` values.
- Kept `--persona-claude` and `--persona-codex` as CSS fallback tokens; custom persona colors now come from pike-agents frontmatter.
- Added Markdown frontmatter persona detection in `metadata_for_file()` for `persona`, `author`, then `agent`, with cache key `(path, mtime, size)` and registry validation.
- Reconciled `ui/src/styles.css` raw color usage through `:root` tokens and documented the full token inventory in `prototypes/visual-system.md`.
- Wrote the implementation report to `docs/active/codex-slice3-report-2026-05-19.md`.

Verification:

- `orb run -m dev sh -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/agent-canvas/ui && pnpm build'` passes. Vite reports the known large-chunk warning.
- `orb run -m dev sh -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/agent-canvas/crates/agent-canvas-app && cargo check'` passes.
- `orb run -m dev sh -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/agent-canvas && cargo test --bin agent-canvas-app 2>&1 | tail -5'` passes: 6 tests.
- `orb run -m dev sh -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/agent-canvas && cargo fmt --all --check'` passes.
- Non-root hex literals in `ui/src/styles.css`: `0`.
- Non-root `rgba(` uses in `ui/src/styles.css`: `0`.

## v0.2-finish Slice 2 Summary

Slice 2 of `docs/BUILD-SPEC-v0.2-finish.md` was implemented on 2026-05-19.

- Removed the fake in-app window chrome; `.main-shell` now fills the native Tauri window.
- Moved Rescan and `+` file ingest actions into the artifact toolbar and kept them visible without requiring an open artifact.
- Wired search filtering for inbox, project, archive, and pinned views, including Esc clearing and Cmd-F focus.
- Added real sidebar project counts via `list_project_counts`.
- Added project row context menu with Open, Rename, and Delete flows; rename/delete are backed by path-bounded Tauri commands and in-app dialogs.
- Added command-palette persona registry reload via `reload_persona_registry`.

Verification:

- `orb run -m dev sh -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/agent-canvas/ui && pnpm build'` passes. Vite reports the known large-chunk warning.
- `orb run -m dev sh -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/agent-canvas/crates/agent-canvas-app && cargo check'` passes.
- `orb run -m dev sh -lc 'cd /mnt/mac/Users/jessepike/code/sandbox/agent-canvas && cargo test --bin agent-canvas-app 2>&1 | tail -10'` passes: 6 tests.

No commit was made; orchestrator commits from host.

## v0.1.1 Patch Summary

AgentCanvas v0.1.1 patch implemented on 2026-05-19 per `docs/PATCH-SPEC-v0.1.1.md`.

Commit/tag blocker: Codex could not write to `.git` in this sandbox. `git add` failed creating `.git/index.lock` with `Operation not permitted`; `touch .git/codex-write-test` also failed. Slice work is present in the working tree, but local atomic commits and `v0.1.1` tag must be created outside this sandbox.

## v0.1.1 Slice Results

| Slice | Result |
|---|---|
| 1 — Restructured payload + dynamic label | Implemented. Payload uses relative path, fenced source, optional `My note`, and `Action:` verb; Rust unit tests cover Markdown and HTML payloads. Button/palette labels now derive from declared agent sessions. |
| 2 — Send popover | Implemented. Send opens a popover with preset/custom action verbs, optional note, Enter-to-send, Esc-to-cancel, and persisted last-used verb in SQLite `settings`. |
| 3 — Default agent per project | Implemented. Added `projects.default_agent_session_id`, right-click agent-card default action, command palette default switch, per-project label resolution, and Shift+Cmd+Enter picker path. |
| 4 — Drag and drop | Implemented. Finder drops copy files to Inbox with collision suffixes; in-app row drag moves files to project/archive with conflict strategy and state row path updates. Drop targets use existing visual-system tokens. |
| 5 — Context menu + open dialog | Implemented. File row context menu includes Open, Toggle Pin, File to Project, Archive, Send to Agent, Reveal in Finder, Copy Path, and Delete. Titlebar `+` opens native file picker and copies to Inbox. |
| 6 — Smoke + release | Partially completed. README Recent updated; crate, UI package, and Tauri versions bumped to 0.1.1. Verification commands are recorded below. Commit and tag are blocked by `.git` write permissions in this sandbox. |

## v0.1.1 Verification

- `orb run -m dev pnpm --dir ui run build` passes. Vite still reports the known large-chunk warning.
- `orb run -m dev cargo fmt --all --check` passes after formatting.
- `orb run -m dev cargo clippy --workspace -- -D warnings` passes.
- `orb run -m dev cargo test --workspace` passes: 64 tests.
- `orb run -m dev cargo run -p vellum-corpus` passes all 67 corpus fixtures byte-identical.
- Local `git tag v0.1.1` failed because `.git/refs/tags/v0.1.1.lock` cannot be created in this sandbox (`Operation not permitted`).

## Summary

AgentCanvas v0.1.0 shipped on 2026-05-19 in place under `~/code/sandbox/vellum/`. The external directory and GitHub repo rename are still external follow-up tasks.

Build time: same-session implementation.
Total new slice commits: 11 before this release-status commit; 12 including this final release commit.

## Slice Results

| Slice | Result |
|---|---|
| 1 — Migration scaffolding | Shipped. Tauri app renamed to `agent-canvas-app`, Vellum v1 docs moved to `legacy/`, UI shell fresh-started from prototypes, project docs updated. |
| 2 — iCloud substrate | Shipped. Bootstraps `AgentCanvas/Inbox`, `Projects/Default`, `Archive`, and SQLite state DB. Real inbox list wired. |
| 3 — Markdown render + save | Shipped. Markdown opens from inbox, renders via parser/ProseMirror, source edit mode saves through atomic stat+hash guard. |
| 4 — HTML render | Shipped. HTML opens in `sandbox="allow-same-origin"` iframe, with source toggle. |
| 5 — Watcher | Shipped. Recursive watcher emits Tauri invalidation events; UI rescans and highlights new files. |
| 6 — Persona registry | Shipped. Reads pike-agents `color:` frontmatter, caches personas in SQLite, falls back to built-ins. |
| 7 — Pasteboard handoff | Shipped. Send-to-Claude formats the v0 payload and writes to `pbcopy` on macOS. |
| 8 — Agent panel | Shipped. Manual sessions persist in SQLite and render persona/backbone/context cards; empty state collapses to gutter. |
| 9 — Command palette | Shipped. Cmd-K palette with actions/files/commands, filtering, arrows, Enter, Esc; Send, Pin, Archive, Open File wired. |
| 10 — Project mode | Shipped. Project folder click switches to three-column layout and opens most recent project artifact. |
| 11 — Polish + smoke | Shipped. j/k, Enter, e, s, p, Cmd-Backspace, /, ?, Cmd-K, Cmd-Enter, and rescan-on-focus wired. Automated substrate checks passed. |
| 12 — README + release | Shipped in final commit. README refreshed and Apache 2.0 retained. |

## Deferrals / Known Issues

- Live MCP server remains deferred to v0.2.
- Comments and anchors remain deferred to v0.2.
- Rendered ProseMirror editing remains out of scope; v0 is source-edit only.
- Three-way merge UI remains out of scope; v0 aborts with a conflict banner.
- PNG/JSON/TXT viewer modes are not implemented.
- The UI bundle is large because ProseMirror and CodeMirror ship in the initial chunk; Vite reports a chunk-size warning, not a build failure.
- Full GUI smoke was not browser-automated from the OrbStack VM. The implemented paths were verified by UI production build, Rust tests, and the format-preservation corpus.

## Acceptance Criteria

| # | Criterion | Status |
|---|---|---|
| 1 | Cold-start bootstraps iCloud folder and shows inbox | Pass by implementation; bootstrap command and app startup create folders. |
| 2 | File round-trip appears via watcher/focus | Pass by implementation; watcher emits events and focus rescans. |
| 3 | Markdown render | Pass by implementation; `.md` opens and renders through parser/ProseMirror. |
| 4 | HTML render | Pass by implementation; `.html` uses sandboxed iframe with scripts disabled. |
| 5 | Source edit + atomic save | Pass by implementation and tests; atomic write path covered. |
| 6 | Optimistic concurrency | Pass by implementation and tests; mismatched base hash aborts. |
| 7 | Agent panel | Pass by implementation; manual session cards persist in SQLite. |
| 8 | Pasteboard handoff | Pass by implementation; `pbcopy` path on macOS, dev fallback elsewhere. |
| 9 | Command palette | Pass by implementation; Cmd-K and Enter action wiring. |
| 10 | Keyboard nav | Pass by implementation; j/k, Enter, p, e wired. |
| 11 | Project mode | Pass by implementation; three-column project layout wired. |
| 12 | Persona colors | Pass by implementation; frontmatter registry + built-in fallback. |
| 13 | Rescan on focus | Pass by implementation; window focus triggers rescan and current-file reload when clean. |

## Verification

- `pnpm --dir ui run build` passes.
- `cargo check --workspace` passes.
- `cargo test --workspace` passes: 62 tests.
- `cargo run -p vellum-corpus` passes: 67/67 fixtures byte-identical.

## Commit List

- `9c87697` — `feat(rename): migrate Vellum scaffolding to AgentCanvas; fresh UI shell`
- `1798c56` — `feat(substrate): wire iCloud folder + SQLite state + inbox list view`
- `f2d93f9` — `feat(viewer): markdown render + edit mode + atomic save`
- `5fda066` — `feat(viewer): sandboxed HTML rendering`
- `d679d28` — `feat(watcher): live file-watch for inbox round-trip`
- `71eb7a7` — `feat(personas): wire persona registry from pike-agents config`
- `d371d00` — `feat(handoff): pasteboard send-to-claude payload`
- `ffdc346` — `feat(ui): agent panel with manual session declaration`
- `8d5ebf3` — `feat(ui): cmd-k command palette with real keyboard wiring`
- `286e1f9` — `feat(ui): two-column inbox + three-column project mode toggle`
- `5cc18be` — `feat(polish): keyboard bindings + rescan-on-focus + smoke test`
## 2026-05-20 — Slice 5e-g implemented by Codex

Implemented AgentCanvas v0.2-finish Slice 5e-g.

- Chose the permitted 5e fallback: Markdown edit mode swaps to CodeMirror source editing and shows "Rendered-view editing lands in v0.3 — using source editor".
- Added source-backed annotation toolbar for Markdown edit mode: bold, italic, strikethrough, code, and mark-for-revision wrappers.
- Added a ProseMirror `revision` mark so mark-for-revision spans render highlighted in Markdown preview.
- Extended the sidecar identity map with optional `base_snapshot` and updated it after successful `write_document` saves.
- Replaced save-conflict banner-only behavior with a 3-column merge dialog showing draft, common ancestor, and current disk source.
- Added merge/revision tokens to `prototypes/visual-system.md` and `ui/src/styles.css`.
- Wrote details to `docs/active/codex-slice5b-report-2026-05-20.md`.

Verification:

- `pnpm install --no-frozen-lockfile` in `ui/` failed because `ui/pnpm-workspace.yaml` has no `packages` field.
- `CI=true pnpm --ignore-workspace install --no-frozen-lockfile` in `ui/` passed.
- Direct `tsc --noEmit` passed.
- `pnpm --ignore-workspace build` in `ui/` passed with the known Vite large-chunk warning.
- `cargo check` in `crates/agent-canvas-app` passed.
- `cargo test --bin agent-canvas-app 2>&1 | tail -5` passed: 6 tests.

## 2026-05-20 — v0.3 Slice 2 implemented by Codex

Implemented interactive HTML, iframe postMessage bridge, and HTML comment anchors.

- Replaced the HTML iframe sandbox with the fixed A22 flags: `allow-scripts allow-forms allow-popups allow-downloads`.
- Added `ui/src/htmlBootstrap.ts` for srcdoc prefix injection. The bootstrap bridges selection changes, iframe console output, iframe-local `Cmd+Shift+M`, `agentcanvas.sendBack`, and scroll-to-snapshot highlighting.
- Extended comments to support legacy text anchors plus `{ kind: "html_selection", start_offset, end_offset, snapshot_text }`.
- Registered `tauri-plugin-persisted-scope` and enabled Tauri `protocol-asset` with `$HOME/**/*` allow plus system-path deny rules.
- Added Rust migration/round-trip tests for legacy and HTML comment anchors.
- Wrote the full implementation report to `docs/active/codex-slice2-v0.3-report-2026-05-20.md`.

Verification:

- `cargo check -q` passes with a non-fatal `ts-rs` serde-attribute warning.
- `cargo test --bin agent-canvas-app 2>&1 | tail -20` passes: 10 tests.
- `./node_modules/.bin/tsc --noEmit` passes.
- `./node_modules/.bin/vite build 2>&1 | tail -5` passes with the known large-chunk warning.
- A22 grep audit: forbidden flags `0`, exact sandbox string `1`.
- Commit attempt failed because this sandbox cannot create `.git/index.lock` (`Operation not permitted`). Working tree contains the completed slice; commit must be created outside the sandbox.

## 2026-05-20 — v0.3 Slice 3 implemented by Codex

Implemented file-level comments and grouped comments UI.

- Added `{ kind: "file_level" }` comment anchors in Rust and TypeScript while preserving legacy markdown text anchors.
- Added "Add comment about this file" toolbar buttons above Markdown, HTML, JSON, TXT, PNG, and PDF viewer bodies.
- Grouped open comments into "Selections" and "About this file" sections, hiding empty headings.
- File-level comment selection now activates the card without invoking source or iframe scroll-to-selection.
- Wrote the full implementation report to `docs/active/codex-slice3-v0.3-report-2026-05-20.md`.

Verification:

- OrbStack dev VM `cargo check -q` in `crates/agent-canvas-app` passes with the existing non-fatal `ts-rs` serde-attribute warning.
- OrbStack dev VM `cargo test --bin agent-canvas-app 2>&1 | tail -20` passes: 11 tests.
- OrbStack dev VM `cargo test 2>&1 | tail -15` in `crates/vellum-core` passes; host run still hits the pre-existing watcher timeout.
- `./node_modules/.bin/tsc --noEmit` passes.
- `./node_modules/.bin/vite build 2>&1 | tail -5` passes with the known large-chunk warning.
- A22 grep audit: forbidden flags `0`.
- A15 raw-hex audit: no output.
