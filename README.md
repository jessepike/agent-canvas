# AgentCanvas

AgentCanvas is a local Mac workbench for reviewing, lightly source-editing, and round-tripping artifacts produced by LLM agents. It watches a plain-file iCloud folder, renders Markdown and HTML, preserves source bytes on save, and copies a formatted handoff payload for Claude/Codex sessions.

This repo was formerly Vellum. The old executable-block editor spec is preserved in `legacy/` only for parser, corpus, atomic-save, and Tauri carry-forward.

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

v0.1.0 ships ad-hoc/dev only. Notarization and signed binaries are deferred.

## License

Apache License 2.0. See `LICENSE`.
