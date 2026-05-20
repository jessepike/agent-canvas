# Codex Slice 1 Report - 2026-05-19

## Files Modified

- `crates/agent-canvas-app/src/main.rs`: Added path-bounding, fallible bootstrap state, identity relink tightening, `last_read_at` update, and Slice 1 tests.
- `docs/active/codex-slice1-report-2026-05-19.md`: Slice 1 implementation and verification report.

## New Helpers / Functions Added

- `AppState::paths()`: Returns live `AgentCanvasPaths` or the captured bootstrap error.
- `AppState::bootstrap_error()`: Exposes captured bootstrap failure for setup event emission.
- `BootstrapErrorPayload`: Payload for the `bootstrap-error` startup event.
- `bootstrap_or_error_state()`: Starts the app with captured bootstrap failure and an in-memory fallback DB instead of panicking.
- `open_in_memory_state_db()`: Initializes fallback SQLite state for bootstrap-failed app startup.
- `initialize_state_db()`: Shared SQLite schema initialization for file and in-memory DBs.
- `path_within_canvas(canvas_root, candidate)`: Canonicalizes root and candidate, resolves non-existent candidates via parent canonicalization, and rejects paths outside AgentCanvas.

## Commands Changed

- `bootstrap_info`: Now returns `Result<BootstrapInfo, String>` and reports bootstrap failure cleanly.
- `list_inbox`, `list_project_files`, `list_archive`, `list_pinned`, `list_projects`, `list_personas`: Gate path access through captured bootstrap state.
- `open_document`: Path-bounded; writes `last_read_at` after successful read.
- `write_document`: Path-bounded before atomic write.
- `load_sidecar`: Path-bounded before sidecar load/migration.
- `save_sidecar`: Path-bounded before sidecar save.
- `archive_file`: Path-bounded source and resolved archive target.
- `toggle_pin`: Path-bounded before state mutation.
- `delete_file`: Path-bounded before file deletion and DB delete.
- `reveal_in_finder`: Path-bounded before Finder reveal.
- `move_file_to_project`: Path-bounded source and resolved project target derivation.
- `move_file_to_archive`: Path-bounded source and archive target derivation.
- `target_file_exists`: Path-bounded resolved project/archive target.
- `copy_paths_to_inbox`: Path-bounded resolved Inbox destination only; incoming source paths remain external.
- `send_to_clipboard`: Path-bounded payload path before formatting.

## Tests Added

- `test_path_within_canvas_rejects_outside`: Rejects `/etc/passwd` and `/tmp/foo`.
- `test_path_within_canvas_accepts_descendant`: Accepts a non-existent `Inbox/x.md` under an existing canvas parent.
- `test_path_within_canvas_resolves_symlinks`: Resolves a symlink candidate to its inside-canvas target.
- `test_identity_relink_skips_when_old_path_exists`: Verifies two same-content files remain separate and do not share pinned state while the old path still exists.

## Verification

- `cd ui && pnpm build && cd ..`: Pass.
- `cd crates/agent-canvas-app && cargo check && cd ../..`: Pass.
- `cargo test --workspace --lib 2>&1 | tail -20`: Pass, 50 passed, 0 failed.
- Extra check: `cargo test -p agent-canvas-app`: Pass, 6 passed, 0 failed, including the new Slice 1 tests.

## Unexpected

- The OrbStack dev VM does not have `zsh`; reran the requested shell sequences under `bash`.
- The requested `cargo test --workspace --lib` command does not execute binary tests in `crates/agent-canvas-app/src/main.rs`, so an additional package test run was needed to exercise the new Slice 1 tests.
