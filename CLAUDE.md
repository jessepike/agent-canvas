# AgentCanvas — Project Context

## What This Is

AgentCanvas is a local Mac workbench for reading, lightly editing, and round-tripping artifacts produced by LLM agents. It is the successor to the earlier Vellum executable-block editor plan. The current v0 target is an artifact inbox rooted in iCloud Drive with Markdown and HTML viewing, source-preserving edits, persona-aware agent context, and pasteboard handoff to Claude/Codex.

## Read First

1. `BUILD-SPEC-v0.md` — canonical implementation plan for AgentCanvas v0.
2. `intent.md` — v2.0 destination and product stance.
3. `prototypes/visual-system.md` — mandatory visual tokens.
4. `prototypes/index.html` plus prototypes A, B, C, D, E, F, I, K — UI target.
5. `legacy/vellum-spec-v0.3.md` — carry-forward only: parser, atomic save guard, format-preservation corpus, Tauri patterns.
6. `decisions.md`, `BACKLOG.md`, `status.md`.

## Current Stage

AgentCanvas v0 implementation. The 12 slices in `BUILD-SPEC-v0.md` are the active plan. The previous Vellum v1.0 plan is abandoned except for reusable file-safety substrate.

## Non-Negotiable Invariants

- Files live under `~/Documents/AgentCanvas/` with subfolders `Inbox/`, `MyFiles/`, `Projects/`, `Archive/`. This path is iCloud-synced via Desktop & Documents integration.
- Files stay plain and source-preserved. No pretty-printing or whitespace normalization.
- Every save carries `base_hash`; mismatches abort with a conflict banner. No last-write-wins.
- Watcher events are UI hints only. Correctness comes from rescan-on-focus and stat+hash before save.
- HTML renders in a sandboxed iframe with scripts disabled by default.
- Persona registry path is configurable; default is `~/code/_shared/pike-agents/plugins/`; missing registry falls back gracefully.
- Visual system tokens in `prototypes/visual-system.md` are authoritative. Do not introduce new colors without updating that file first.

## Out Of Scope For v0

- Live MCP server or socket protocol.
- Comments, anchors, pending-review workflows.
- Rendered ProseMirror editing.
- Three-way merge UI.
- Annotation toolbar.
- PNG, JSON, TXT viewer modes.

## Working Agreements

- Implement slices in order and commit atomically using conventional commits.
- Keep package installs, builds, test runners, and dev servers in the OrbStack dev VM.
- Do not modify `intent.md` unless Jesse explicitly asks.
- Do not modify `legacy/vellum-spec-v0.3.md`; it is carry-forward reference only.
- Update `status.md` before ending any file-changing session.
