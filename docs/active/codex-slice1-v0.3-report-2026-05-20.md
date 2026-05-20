# v0.3 Slice 1 — Flavor 2 Data Model Report

## Files Modified

- `crates/agent-canvas-app/src/main.rs`
- `ui/src/App.tsx`
- `ui/src/ipc.ts`
- `ui/src/styles.css`
- `status.md`
- `lessons.md`

## DB Migrations Run

- Added idempotent `files.in_inbox INTEGER NOT NULL DEFAULT 0`.
- Added idempotent `files.project_tag TEXT`.
- Added idempotent `files.archived INTEGER NOT NULL DEFAULT 0` guard for older DBs.
- Backfill reads legacy iCloud paths under `~/Library/Mobile Documents/com~apple~CloudDocs/AgentCanvas/` and sets Inbox, Project, and Archive tags only for untagged matching rows.

## New Tauri Commands

- `track_paths_in_inbox(paths)` tracks absolute source paths in place with `in_inbox = 1`.
- `untrack_file(path)` removes the DB row and leaves the disk file untouched.
- `delete_file_from_disk(path)` removes the disk file and deletes the DB row.

## Removed/Renamed Commands

- `copy_paths_to_inbox(paths)` remains as a deprecated compatibility alias to `track_paths_in_inbox`.
- `delete_file(path)` remains as a deprecated compatibility alias to `delete_file_from_disk`.
- `move_file_to_project` and `move_file_to_archive` keep their signatures but now update DB tags only.

## Tests Added

- `path_safe_for_canvas` allow/deny matrix.
- Legacy-path tag migration idempotency.
- `untrack_file` vs `delete_file_from_disk` behavior split.

## Verification Results

- `cd crates/agent-canvas-app && cargo check -q` — passed.
- `cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -10` — passed: 8 tests, 0 failed.
- `cd ui && ./node_modules/.bin/tsc --noEmit` — passed.
- `cd ui && ./node_modules/.bin/vite build 2>&1 | tail -5` — passed with known large-chunk warning.
- CSS raw-hex regression command printed `0`.

Note: the first Vite run failed because Rollup's Linux ARM64 optional native package was missing from `node_modules`. `CI=true pnpm install --frozen-lockfile` was run inside the dev VM to refresh dependencies, then Vite passed.

## Invariant Audit

- A12 relaxed: path-touching commands now route through `path_safe_for_canvas` before filesystem work.
- A14: persona registry/color code was not changed.
- A15: raw hex outside `:root` remains `0`.
- A16: no fake window chrome changes.
- A17: no `window.prompt` / `window.confirm`; destructive disk delete uses `<ConfirmDialog>`.

## Known Issues/Gaps

- `target_file_exists` now returns `false` for project/archive tagging because no filesystem move occurs and path collisions are irrelevant.
- Existing project rename/delete flows were converted to tag metadata operations; legacy project folders are no longer authoritative.
- `archive_file` remains for toolbar compatibility, but now tags the file as archived rather than moving it.
