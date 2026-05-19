---
project: agent-canvas
updated: 2026-05-19
stage: v0.1.0 shipped
---

# AgentCanvas — Status

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
