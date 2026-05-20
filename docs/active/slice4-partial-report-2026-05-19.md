# Slice 4 (partial) — Implemented by Claude after Codex rate limit

**Trigger:** Codex hit GPT-5 usage limit mid-orchestration. Resume time ~8:54 PM PDT.
Claude implemented the highest-value Slice 4 items inline to keep momentum.

## Landed in this commit

### 4a — File rename (full)
- New Rust command `rename_file(state, old_path, new_name) -> FileMetadata` — path-bound,
  refuses on `/`, `\\`, `..`, empty, or existing target. fs::rename + DB path update.
- ipc.ts wrapper `renameFile()`.
- New `<RenameFileDialog>` React component — focus + select-without-extension on mount,
  Enter confirms, Esc cancels.
- Context menu entry "Rename... (F2)" between Toggle Pin and File to Project.
- F2 keyboard shortcut wired (with a file selected, opens the dialog).
- Toast: "Renamed → {newName}" on success.
- Artifact + selectedPath stays attached to the new path post-rename.

### 4d — In-app conflict modal (full)
- `<ConflictDialog>` React component with three options:
  Cancel / Replace (destructive styling) / Keep Both (default, primary).
- Enter = Keep Both, Esc = Cancel.
- `conflictStrategyForTarget()` rewritten to call back into a React-state-managed
  Promise-resolver pattern (`openConflictDialog`).
- All four callers updated: moveInboxFileToProject, moveKnownFileToProject,
  moveInboxFileToArchive, moveKnownFileToArchive.
- The remaining `window.prompt` in the codebase is the default-agent-switcher
  at line ~544; deferred to Slice 6 polish (`<AgentPickerDialog>`).

### Rust backend ready for 4c — Multi-file Send
- New command `send_multi_to_clipboard(state, payloads: Vec<SendPayload>) -> String`
  — path-bound per payload, builds the multi-file format from the spec.
- ipc.ts wrapper `sendMultiToClipboard()`.
- New helper `format_send_multi_payload()` in main.rs.
- Backend test coverage: existing send tests still pass; multi-send has no
  dedicated test yet (covered when UI lands).
- UI wiring pending (Codex resume task).

## Deferred for Codex resume

### 4b — Multi-file selection (UI)
- `selectedPaths: Set<string>` state
- ⌘-click / shift-click handlers on file rows in inbox/project/archive/pinned lists
- `.file-row.multi-selected` CSS variant
- Bulk-action placeholder in the content pane when N > 1

### 4c — Multi-file Send (UI)
- Send popover header: "Send N files to {agent}" when N > 1
- Wire send through `sendMultiToClipboard` (backend ready)
- Toast: "Copied N files to clipboard for {agent}"

### 4e — Drag-out to Finder
- Requires `tauri-plugin-drag` Cargo dep + `@tauri-apps/plugin-drag` npm dep
- New ipc command + dragstart handler
- Falls back to context-menu "Export to..." with native save dialog if plugin
  proves problematic
- Both paths require touching Cargo.toml + package.json; deferred to avoid
  mid-orchestration dependency drift

### 4d-extra — Agent picker dialog
- Replace remaining `window.prompt` for default-agent switching at App.tsx:544
- `<AgentPickerDialog>` with radio list of sessions
- Could fold into Slice 6 polish

## Verification

- `cd ui && pnpm build` — ✓ built in 916ms
- `cd crates/agent-canvas-app && cargo check` — clean (10s)
- `cargo test --bin agent-canvas-app` — 6 passed, 0 failed

## Notes for resuming Codex

The Slice 4 spec in `docs/BUILD-SPEC-v0.2-finish.md` still describes the full
scope. When resuming, focus Codex on the four deferred items above. All
relevant Rust backend (rename_file, send_multi_to_clipboard) is already in
place — Codex only needs the UI work + the tauri-plugin-drag installation
(or the Export-to-Finder fallback).
