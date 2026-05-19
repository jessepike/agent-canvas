# AgentCanvas

AgentCanvas is a local Mac workbench for reviewing, lightly source-editing, and round-tripping artifacts produced by LLM agents. It watches a plain-file iCloud folder, renders Markdown and HTML, preserves source bytes on save, and copies a formatted handoff payload for Claude/Codex sessions.

This repo was formerly Vellum. The old executable-block editor spec is preserved in `legacy/` only for parser, corpus, atomic-save, and Tauri carry-forward.

## Recent

v0.1.1 tightens the first real-use loop:

- Send-to-Agent now copies a prompt-ready payload with a relative AgentCanvas path, fenced source, optional note, and explicit action verb.
- Send opens a popover with Review/Revise/Expand/Critique/Summarize/Respond-to/custom action choices.
- Agent session labels are dynamic, with per-project defaults and a picker shortcut for multi-agent sessions.
- Finder files can be dropped into Inbox; Inbox rows can be dragged to Projects or Archive.
- File rows now have a right-click menu for open, pin, file-to-project, archive, send, reveal, copy relative path, and delete.
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

v0.1.1 ships ad-hoc/dev only. Notarization and signed binaries are deferred.

## License

Apache License 2.0. See `LICENSE`.
