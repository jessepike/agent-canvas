# v0.3 Watcher Coverage Fix Report

## 1. Files Modified

- `crates/vellum-core/src/watch/mod.rs`
- `crates/agent-canvas-app/src/main.rs`
- `crates/agent-canvas-app/src/mcp/mod.rs`
- `BACKLOG.md`
- `status.md`

## 2. New WatchHandle API

```rust
pub fn start(
    callback: impl Fn(WatchEvent) + Send + Sync + 'static,
) -> Result<WatchHandle, WatchError>;

impl WatchHandle {
    pub fn add_path(&self, path: &Path) -> Result<(), WatchError>;
    pub fn remove_path(&self, path: &Path) -> Result<(), WatchError>;
    pub fn set_paths(&self, paths: Vec<PathBuf>) -> Result<(), WatchError>;
    pub fn watch_recursive(&self, root: &Path) -> Result<(), WatchError>;
}
```

`watch_vault(&Path, callback)` remains as a compatibility wrapper around `start(...)` plus `watch_recursive(...)`.

## 3. Parent-Dir Ref-Counting Strategy

```rust
let Some(parent) = path.parent().map(Path::to_path_buf) else {
    return Ok(());
};
let mut watched_dirs = self
    .watched_dirs
    .lock()
    .expect("watch directory map lock poisoned");
let count = watched_dirs.entry(parent.clone()).or_insert(0);
if *count == 0 {
    if let Some(watcher) = self.watcher.lock().expect("watcher lock poisoned").as_mut() {
        watcher.watch(&parent, RecursiveMode::NonRecursive)?;
    }
}
*count += 1;
```

Removal decrements the same parent count and calls `watcher.unwatch(&parent)` when the count reaches zero. Paths are normalized before matching to avoid macOS `/var` versus `/private/var` misses. The worker also performs a lightweight snapshot poll on the existing 200ms cadence so macOS notify timing does not make tests or push delivery nondeterministic.

## 4. Watcher Re-Sync Wiring Points

`main.rs` now starts the watcher with `watch::start(...)`, calls `watcher.watch_recursive(&canvas_root)`, loads initial tracked paths with:

```sql
SELECT path FROM files
WHERE in_inbox = 1
   OR project_tag IS NOT NULL
   OR archived = 1
   OR pinned = 1
```

and calls `watcher.set_paths(...)`.

Commands now calling `resync_watcher_from_db` after DB/path membership changes:

- `track_paths_in_inbox`
- `untrack_file`
- `move_file_to_project`
- `move_file_to_archive`
- `delete_file_from_disk`
- `archive_file`
- `toggle_pin`
- `rename_file`

## 5. New Tests

- `vellum_core::watch::tests::watch_accepts_slice3_extensions` — pass
- `vellum_core::watch::tests::add_path_then_modify_emits_changed_for_arbitrary_location` — pass
- `vellum_core::watch::tests::remove_path_stops_subsequent_events` — pass
- `vellum_core::watch::tests::set_paths_replaces_set_atomically` — pass
- `agent_canvas_app::mcp::tests::watcher_change_dispatches_artifact_updated_notification` — pass

Existing watcher tests also pass, including `watch_emits_event_on_modify`, `watch_filters_non_md_files`, `watch_ignores_vellum_tmp_files`, and `watch_ignores_vellum_cache`.

## 6. Verification Output

```text
$ cd crates/vellum-core && cargo test 2>&1 | tail -20
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

Doc-tests vellum_core
warning: failed to parse serde attribute
  |
  | #[serde(skip_serializing_if = "Option::is_none")]
  |
  = note: ts-rs failed to parse this attribute. It will be ignored.

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cd crates/agent-canvas-app && cargo check -q
warning: failed to parse serde attribute
  |
  | #[serde(skip_serializing_if = "Option::is_none")]
  |
  = note: ts-rs failed to parse this attribute. It will be ignored.

$ cd crates/agent-canvas-app && cargo test --bin agent-canvas-app 2>&1 | tail -25
test mcp::tests::watcher_change_dispatches_artifact_updated_notification ... ok

test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.43s

$ cd crates/agent-canvas-mcp && cargo build
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.04s

$ cd ui && ./node_modules/.bin/tsc --noEmit
pass

$ cd ui && ./node_modules/.bin/vite build 2>&1 | tail -3
- Use build.rollupOptions.output.manualChunks to improve chunking: https://rollupjs.org/configuration-options/#output-manualchunks
- Adjust chunk size limit for this warning via build.chunkSizeWarningLimit.
✓ built in 927ms
```
