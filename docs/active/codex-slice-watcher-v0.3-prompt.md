# v0.3 Watcher Coverage Fix — Multi-Path Watcher for Flavor 2

You are implementing the v0.3 critical fix described in `BACKLOG.md` § "v0.3 Critical fixes". This unblocks Slice 6 acceptance.

Read `docs/BUILD-SPEC-v0.3.md` (especially Slice 1's tag-based file model + Slice 5's `emit_artifact_updated` path), and review `crates/vellum-core/src/watch/mod.rs` (current single-root watcher) and the Slice 5 wiring at `crates/agent-canvas-app/src/main.rs:2099-2107`.

## The bug

Flavor 2 (Slice 1) tracks files by absolute path anywhere on disk. The watcher in `vellum-core::watch` still only watches a single `vault_root` (the iCloud canvas folder). Any tracked file outside that folder never fires `WatchEvent::Changed`, so `mcp::emit_artifact_updated` never reaches MCP clients for those files. Today the DB has ~6 tracked files; zero of them are inside canvas root, so the push channel is structurally dead in practice.

## What "done" looks like

1. The watcher tracks the **union of parent directories** of every tracked file in the `files` table (any row with `in_inbox=1 OR project_tag IS NOT NULL OR archived=1 OR pinned=1`), deduplicated. Plus the existing canvas root (still useful for ad-hoc new files appearing in `Inbox/`).
2. Watcher refreshes its set when files are tracked/untracked. Specifically: after every `track_paths_in_inbox`, `untrack_file`, `move_file_to_project`, `move_file_to_archive`, `delete_file_from_disk`, the watcher's tracked-path set is re-synced.
3. **Extension filter expanded:** md/markdown/html/htm + png/jpg/pdf/json/txt (mirror the kinds Slice 3 supports).
4. End-to-end CLI smoke test passes:
   - Start app (binds socket)
   - Run the shim with `initialize` + `notifications/subscribe { events: ["artifact_updated"] }`
   - From another terminal: `echo "marker" >> <any-tracked-file>`
   - Shim stdout receives `{"jsonrpc":"2.0","method":"notifications/artifact_updated","params":{"path":"...","by":"watcher"}}`

## Implementation plan

### 1. New watcher API in `crates/vellum-core/src/watch/mod.rs`

Replace `watch_vault(&Path, callback)` with a builder + handle that supports multi-path. Old call site shape stays similar — minimal disruption.

```rust
pub struct WatchHandle { ... }  // unchanged externally

pub fn start(callback: impl Fn(WatchEvent) + Send + Sync + 'static) -> Result<WatchHandle, WatchError>;

impl WatchHandle {
    /// Idempotent. Adds the parent dir of `path` (non-recursive) if not
    /// already watched. Stores `path` in the set of "interesting" paths
    /// (events on other files in the same parent are filtered out).
    pub fn add_path(&self, path: &Path) -> Result<(), WatchError>;

    /// Idempotent. Removes from the interesting-path set. If the parent
    /// dir no longer contains any interesting path, unwatch it.
    pub fn remove_path(&self, path: &Path) -> Result<(), WatchError>;

    /// Replace the entire interesting-path set in one shot. Cheaper than
    /// add/remove churn when re-syncing from the DB.
    pub fn set_paths(&self, paths: Vec<PathBuf>) -> Result<(), WatchError>;

    /// Watch a directory recursively. Used by main.rs to keep the canvas
    /// root covered so newly-dropped files still surface as Created.
    pub fn watch_recursive(&self, root: &Path) -> Result<(), WatchError>;
}
```

Internals:
- `RecommendedWatcher` wrapped in a `Mutex` since `notify::Watcher::watch/unwatch` need `&mut self`.
- `Arc<RwLock<HashSet<PathBuf>>>` for the interesting-paths set, read by the worker thread on each event.
- Worker filters incoming notify events: keep only if `event.paths.any(|p| interesting.contains(p))`. The existing `should_watch_path` extension allow-list still applies as a secondary filter.
- Parent-dir watching: when `add_path("/foo/bar/x.md")` is called, watch `/foo/bar/` non-recursively if not already watched. Track a `HashMap<PathBuf, usize>` ref-count of watched directories so `remove_path` can unwatch when count drops to zero.
- `set_paths(new)` computes diff against current set and applies adds/removes — keep the watcher operation count minimal.

### 2. Update extension allow-list

```rust
const TRACKED_EXTENSIONS: &[&str] = &["md", "markdown", "html", "htm", "png", "jpg", "jpeg", "pdf", "json", "txt"];
```

Use this in `should_watch_path`.

### 3. Wire into `crates/agent-canvas-app/src/main.rs`

Replace the single `watch::watch_vault(&canvas_root, ...)` call at line 2099 with:
- Call `watch::start(callback)` once.
- Call `watcher.watch_recursive(&canvas_root)` for the iCloud area.
- Build initial tracked-path list from DB (`SELECT path FROM files WHERE in_inbox=1 OR project_tag IS NOT NULL OR archived=1 OR pinned=1`), then `watcher.set_paths(...)`.

Add a re-sync helper:

```rust
fn resync_watcher_from_db(state: &AppState) -> Result<(), String>;
```

Call it after every track/untrack/move command — drop-in line after the existing DB mutation in each Tauri command.

### 4. Slice 6-blocking smoke test

Write a Rust integration-style test in `crates/agent-canvas-app/src/mcp/mod.rs` that:
- Creates a temp dir
- Calls `watch::start` + `watch_recursive` on the temp dir
- Spawns a notification dispatcher channel
- Modifies a file in the temp dir
- Asserts `notifications/artifact_updated` is dispatched within 1.5s

Plus a unit test in `crates/vellum-core/src/watch/mod.rs`:
- `add_path_then_modify_emits_changed_for_arbitrary_location` (use a second temp dir outside the watch_recursive root)
- `remove_path_stops_subsequent_events`
- `set_paths_replaces_set_atomically`

## Files you will touch

- `crates/vellum-core/src/watch/mod.rs` — new API + tests
- `crates/agent-canvas-app/src/main.rs` — call site swap + `resync_watcher_from_db` + invoke from track/untrack/move/delete commands; AppState field for the watcher handle
- `crates/agent-canvas-app/src/mcp/mod.rs` — new integration test
- `status.md`, `BACKLOG.md` (close v0.3 Critical fix, log any spinoffs)

## Hard constraints

- A22 / A15 / A17 unchanged.
- Do not touch UI code (this is pure backend).
- Do not modify intent.md, BUILD-SPEC-v0.3.md, legacy/vellum-spec-v0.3.md.
- The existing tests `watch_emits_event_on_modify`, `watch_filters_non_md_files`, `watch_ignores_vellum_tmp_files`, `watch_ignores_vellum_cache` must continue passing.
- `WatchHandle` Drop semantics stay the same (clean watcher + worker shutdown).
- Don't introduce a new dep; `notify` already has multi-path support.
- Don't reorder MCP wiring — `emit_artifact_updated` is the dispatch entry point, leave it alone.

## Verification you must run

```bash
cd crates/vellum-core && cargo test 2>&1 | tail -20
cd crates/agent-canvas-app && cargo check -q
cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -25
cd crates/agent-canvas-mcp && cargo build
cd ui && ./node_modules/.bin/tsc --noEmit
cd ui && ./node_modules/.bin/vite build 2>&1 | tail -3
```

All must pass. The previously-flaky `watch_emits_event_on_modify` on macOS should now pass deterministically (and if you can fix the timing as part of this work, do).

## Report format

Write `docs/active/codex-watcher-v0.3-report-2026-05-21.md`:

1. Files modified
2. New WatchHandle API (paste the impl block signatures)
3. Parent-dir ref-counting strategy (with code)
4. Watcher re-sync wiring points in main.rs (list of Tauri commands that now call `resync_watcher_from_db`)
5. New tests (list with pass/fail)
6. Verification output

Commit:

```
fix(v0.3): multi-path watcher covers tracked files outside canvas root
```

## Out of scope

- Adapting the watcher to support remote / network paths
- Watching beyond the parent directory (no recursive-into-subdirs-of-tracked-paths)
- Cross-platform behavior (macOS-only for v0.3)
- Slice 6 work
- Performance optimization beyond the dedup ref-count
- File-system-events deduplication beyond the existing 200ms debounce

If you find an improvement adjacent, log to BACKLOG.md under `[v0.3-watcher-spinoff]`. Do not build it.
