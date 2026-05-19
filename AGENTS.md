# AgentCanvas — Codex Context

## Read First

1. `BUILD-SPEC-v0.md`
2. `intent.md`
3. `prototypes/visual-system.md`
4. `prototypes/index.html` plus prototypes A, B, C, D, E, F, I, K
5. `legacy/vellum-spec-v0.3.md` for carry-forward only
6. `CLAUDE.md`, `decisions.md`, `BACKLOG.md`, `status.md`

## Active Scope

AgentCanvas v0.1.0 is an iCloud-backed artifact workbench for reviewing, lightly source-editing, and round-tripping Markdown and HTML artifacts produced by LLM agents.

The build sequence in `BUILD-SPEC-v0.md` is canonical. The earlier Vellum v1.0 executable-block editor plan is abandoned, except for the Rust parser, format-preservation corpus, atomic save guard, watcher patterns, and Tauri IPC substrate.

## Hard Rules

- Files shown by the app must live under `~/iCloud/AgentCanvas/`, backed by the real iCloud Drive path.
- Edits must preserve source bytes except where the user edited.
- Saves use optimistic concurrency with `base_hash`; mismatches abort.
- Watcher events are hints. Rescan on focus and before open/save.
- HTML renders in a sandboxed iframe with scripts disabled by default.
- Persona registry reads are configurable and fall back gracefully.
- Visual tokens come from `prototypes/visual-system.md`.

## v0 Deferrals

- Live MCP server
- Comments and anchors
- Rendered ProseMirror editing
- Three-way merge UI
- Annotation toolbar
- PNG/JSON/TXT viewer modes

## Commit Discipline

Use conventional commits after each completed slice. Commit bodies for this build include:

```
Implemented-by: Codex (GPT Pro)
Planned-by: Claude (Opus 4.7)
Co-Authored-By: Codex <noreply@openai.com>
```
