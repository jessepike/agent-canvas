use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, RwLock, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, UNIX_EPOCH},
};

use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{ModifyKind, RenameMode},
};
use thiserror::Error;

const MODIFY_DEBOUNCE: Duration = Duration::from_millis(200);
const TRACKED_EXTENSIONS: &[&str] = &[
    "md", "markdown", "html", "htm", "png", "jpg", "jpeg", "pdf", "json", "txt",
];

#[derive(Debug, Error)]
pub enum WatchError {
    #[error("notify error: {0}")]
    Notify(#[from] notify::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    /// File contents changed externally (not from a Vellum atomic_write).
    Changed {
        path: PathBuf,
        on_disk_hash: [u8; 32],
    },
    /// File was created.
    Created { path: PathBuf },
    /// File was removed.
    Removed { path: PathBuf },
    /// File was renamed (when notify can disambiguate from create+remove).
    Renamed { from: PathBuf, to: PathBuf },
}

/// Opaque watcher handle. Dropping it stops the underlying watcher thread.
pub struct WatchHandle {
    watcher: Mutex<Option<RecommendedWatcher>>,
    worker: Option<JoinHandle<()>>,
    interesting_paths: Arc<RwLock<HashSet<PathBuf>>>,
    recursive_roots: Arc<RwLock<HashSet<PathBuf>>>,
    snapshots: Arc<Mutex<HashMap<PathBuf, Option<FileSnapshot>>>>,
    watched_dirs: Mutex<HashMap<PathBuf, usize>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileSnapshot {
    size: u64,
    modified_nanos: u128,
}

pub fn start(
    callback: impl Fn(WatchEvent) + Send + Sync + 'static,
) -> Result<WatchHandle, WatchError> {
    let (event_tx, event_rx) = mpsc::channel();
    let interesting_paths = Arc::new(RwLock::new(HashSet::new()));
    let recursive_roots = Arc::new(RwLock::new(HashSet::new()));
    let snapshots: Arc<Mutex<HashMap<PathBuf, Option<FileSnapshot>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let watcher = notify::recommended_watcher(move |result| {
        if event_tx.send(result).is_err() {
            tracing::warn!("watch event received after worker stopped");
        }
    })?;

    let worker_interesting_paths = Arc::clone(&interesting_paths);
    let worker_recursive_roots = Arc::clone(&recursive_roots);
    let worker_snapshots = Arc::clone(&snapshots);
    let worker = thread::spawn(move || {
        run_watch_worker(
            event_rx,
            worker_interesting_paths,
            worker_recursive_roots,
            worker_snapshots,
            callback,
        );
    });

    Ok(WatchHandle {
        watcher: Mutex::new(Some(watcher)),
        worker: Some(worker),
        interesting_paths,
        recursive_roots,
        snapshots,
        watched_dirs: Mutex::new(HashMap::new()),
    })
}

pub fn watch_vault(
    vault_root: &Path,
    callback: impl Fn(WatchEvent) + Send + Sync + 'static,
) -> Result<WatchHandle, WatchError> {
    let handle = start(callback)?;
    handle.watch_recursive(vault_root)?;
    Ok(handle)
}

impl WatchHandle {
    /// Idempotent. Adds the parent dir of `path` non-recursively if it is not
    /// already watched, and stores `path` in the interesting-path set.
    pub fn add_path(&self, path: &Path) -> Result<(), WatchError> {
        let path = normalize_watch_path(path);
        {
            let mut interesting_paths = self
                .interesting_paths
                .write()
                .expect("watch interesting paths lock poisoned");
            if !interesting_paths.insert(path.clone()) {
                return Ok(());
            }
        }
        self.snapshots
            .lock()
            .expect("watch snapshots lock poisoned")
            .insert(path.clone(), snapshot_for_path(&path));

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
        Ok(())
    }

    /// Idempotent. Removes `path` from the interesting-path set and unwatches
    /// its parent directory when no interesting paths remain there.
    pub fn remove_path(&self, path: &Path) -> Result<(), WatchError> {
        let path = normalize_watch_path(path);
        {
            let mut interesting_paths = self
                .interesting_paths
                .write()
                .expect("watch interesting paths lock poisoned");
            if !interesting_paths.remove(&path) {
                return Ok(());
            }
        }
        self.snapshots
            .lock()
            .expect("watch snapshots lock poisoned")
            .remove(&path);

        let Some(parent) = path.parent().map(Path::to_path_buf) else {
            return Ok(());
        };
        let mut watched_dirs = self
            .watched_dirs
            .lock()
            .expect("watch directory map lock poisoned");
        let Some(count) = watched_dirs.get_mut(&parent) else {
            return Ok(());
        };
        *count = count.saturating_sub(1);
        if *count == 0 {
            watched_dirs.remove(&parent);
            if let Some(watcher) = self.watcher.lock().expect("watcher lock poisoned").as_mut() {
                watcher.unwatch(&parent)?;
            }
        }
        Ok(())
    }

    /// Replace the full interesting-path set, applying only the add/remove
    /// operations needed to reach the requested state.
    pub fn set_paths(&self, paths: Vec<PathBuf>) -> Result<(), WatchError> {
        let next: HashSet<PathBuf> = paths
            .into_iter()
            .map(|path| normalize_watch_path(&path))
            .collect();
        let current = self
            .interesting_paths
            .read()
            .expect("watch interesting paths lock poisoned")
            .clone();

        for path in current.difference(&next) {
            self.remove_path(path)?;
        }
        for path in next.difference(&current) {
            self.add_path(path)?;
        }
        Ok(())
    }

    /// Watch a directory recursively. Recursive roots pass the worker filter
    /// without being listed as explicit interesting paths.
    pub fn watch_recursive(&self, root: &Path) -> Result<(), WatchError> {
        let root = normalize_watch_path(root);
        {
            let mut recursive_roots = self
                .recursive_roots
                .write()
                .expect("watch recursive roots lock poisoned");
            if !recursive_roots.insert(root.clone()) {
                return Ok(());
            }
        }
        seed_recursive_snapshots(&root, &self.snapshots);
        if let Some(watcher) = self.watcher.lock().expect("watcher lock poisoned").as_mut() {
            watcher.watch(&root, RecursiveMode::Recursive)?;
        }
        Ok(())
    }
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        if let Ok(mut watcher) = self.watcher.lock() {
            drop(watcher.take());
        }
        if let Some(worker) = self.worker.take()
            && let Err(error) = worker.join()
        {
            tracing::warn!(?error, "watch worker thread panicked");
        }
    }
}

fn run_watch_worker(
    event_rx: mpsc::Receiver<notify::Result<Event>>,
    interesting_paths: Arc<RwLock<HashSet<PathBuf>>>,
    recursive_roots: Arc<RwLock<HashSet<PathBuf>>>,
    snapshots: Arc<Mutex<HashMap<PathBuf, Option<FileSnapshot>>>>,
    callback: impl Fn(WatchEvent),
) {
    let mut pending_changed = HashSet::new();

    loop {
        match event_rx.recv_timeout(MODIFY_DEBOUNCE) {
            Ok(Ok(event)) => {
                process_notify_event(
                    event,
                    &interesting_paths,
                    &recursive_roots,
                    &callback,
                    &mut pending_changed,
                );
            }
            Ok(Err(error)) => {
                tracing::warn!(?error, "file watch backend error");
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                flush_changed(&mut pending_changed, &snapshots, &callback);
                poll_watch_state(&interesting_paths, &recursive_roots, &snapshots, &callback);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                flush_changed(&mut pending_changed, &snapshots, &callback);
                break;
            }
        }
    }
}

fn process_notify_event(
    event: Event,
    interesting_paths: &Arc<RwLock<HashSet<PathBuf>>>,
    recursive_roots: &Arc<RwLock<HashSet<PathBuf>>>,
    callback: &impl Fn(WatchEvent),
    pending_changed: &mut HashSet<PathBuf>,
) {
    match event.kind {
        EventKind::Create(_) => {
            for path in filter_paths(event.paths, interesting_paths, recursive_roots) {
                callback(WatchEvent::Created { path });
            }
        }
        EventKind::Remove(_) => {
            for path in filter_paths(event.paths, interesting_paths, recursive_roots) {
                callback(WatchEvent::Removed { path });
            }
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() >= 2 => {
            let from = event.paths[0].clone();
            let to = event.paths[1].clone();
            if should_watch_path(&from, interesting_paths, recursive_roots)
                && should_watch_path(&to, interesting_paths, recursive_roots)
            {
                callback(WatchEvent::Renamed { from, to });
            }
        }
        EventKind::Modify(_) => {
            // Backends and editors often emit several modify notifications for
            // one user-visible write. Debounce at the worker boundary so the UI
            // sees one Changed event per path after the write settles.
            pending_changed.extend(filter_paths(
                event.paths,
                interesting_paths,
                recursive_roots,
            ));
        }
        _ => {}
    }
}

fn flush_changed(
    pending_changed: &mut HashSet<PathBuf>,
    snapshots: &Arc<Mutex<HashMap<PathBuf, Option<FileSnapshot>>>>,
    callback: &impl Fn(WatchEvent),
) {
    for path in pending_changed.drain() {
        match fs::read(&path) {
            Ok(contents) => {
                let on_disk_hash = *blake3::hash(&contents).as_bytes();
                snapshots
                    .lock()
                    .expect("watch snapshots lock poisoned")
                    .insert(path.clone(), snapshot_for_path(&path));
                callback(WatchEvent::Changed { path, on_disk_hash });
            }
            Err(error) => {
                tracing::warn!(?path, ?error, "could not hash changed file");
            }
        }
    }
}

fn poll_watch_state(
    interesting_paths: &Arc<RwLock<HashSet<PathBuf>>>,
    recursive_roots: &Arc<RwLock<HashSet<PathBuf>>>,
    snapshots: &Arc<Mutex<HashMap<PathBuf, Option<FileSnapshot>>>>,
    callback: &impl Fn(WatchEvent),
) {
    let interesting_paths = interesting_paths
        .read()
        .expect("watch interesting paths lock poisoned")
        .clone();
    let recursive_roots = recursive_roots
        .read()
        .expect("watch recursive roots lock poisoned")
        .clone();
    let mut current_paths = interesting_paths.clone();
    for root in &recursive_roots {
        collect_recursive_paths(root, &mut current_paths);
    }

    let mut snapshots = snapshots.lock().expect("watch snapshots lock poisoned");
    let previous_paths: Vec<PathBuf> = snapshots.keys().cloned().collect();
    for path in previous_paths {
        if !current_paths.contains(&path) {
            snapshots.remove(&path);
            continue;
        }
        let previous = snapshots.get(&path).copied().flatten();
        let current = snapshot_for_path(&path);
        if previous == current {
            continue;
        }
        match (previous, current) {
            (Some(_), Some(snapshot)) => {
                snapshots.insert(path.clone(), Some(snapshot));
                emit_changed(&path, callback);
            }
            (None, Some(snapshot)) => {
                snapshots.insert(path.clone(), Some(snapshot));
                callback(WatchEvent::Created { path });
            }
            (Some(_), None) => {
                snapshots.insert(path.clone(), None);
                callback(WatchEvent::Removed { path });
            }
            (None, None) => {}
        }
    }

    for path in current_paths {
        if snapshots.contains_key(&path) {
            continue;
        }
        let snapshot = snapshot_for_path(&path);
        snapshots.insert(path.clone(), snapshot);
        if snapshot.is_some() {
            callback(WatchEvent::Created { path });
        }
    }
}

fn collect_recursive_paths(root: &Path, paths: &mut HashSet<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = normalize_watch_path(&entry.path());
        if path.components().any(
            |component| matches!(component, Component::Normal(name) if name == ".vellum-cache"),
        ) {
            continue;
        }
        if path.is_dir() {
            collect_recursive_paths(&path, paths);
        } else if supported_extension(&path) && !is_vellum_tmp_path(&path) {
            paths.insert(path);
        }
    }
}

fn seed_recursive_snapshots(
    root: &Path,
    snapshots: &Arc<Mutex<HashMap<PathBuf, Option<FileSnapshot>>>>,
) {
    let mut paths = HashSet::new();
    collect_recursive_paths(root, &mut paths);
    let mut snapshots = snapshots.lock().expect("watch snapshots lock poisoned");
    for path in paths {
        snapshots.insert(path.clone(), snapshot_for_path(&path));
    }
}

fn snapshot_for_path(path: &Path) -> Option<FileSnapshot> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    Some(FileSnapshot {
        size: metadata.len(),
        modified_nanos,
    })
}

fn emit_changed(path: &Path, callback: &impl Fn(WatchEvent)) {
    match fs::read(path) {
        Ok(contents) => {
            let on_disk_hash = *blake3::hash(&contents).as_bytes();
            callback(WatchEvent::Changed {
                path: path.to_path_buf(),
                on_disk_hash,
            });
        }
        Err(error) => {
            tracing::warn!(?path, ?error, "could not hash changed file");
        }
    }
}

fn filter_paths<'a>(
    paths: Vec<PathBuf>,
    interesting_paths: &'a Arc<RwLock<HashSet<PathBuf>>>,
    recursive_roots: &'a Arc<RwLock<HashSet<PathBuf>>>,
) -> impl Iterator<Item = PathBuf> + 'a {
    paths
        .into_iter()
        .map(|path| normalize_watch_path(&path))
        .filter(move |path| should_watch_path(path, interesting_paths, recursive_roots))
}

fn should_watch_path(
    path: &Path,
    interesting_paths: &Arc<RwLock<HashSet<PathBuf>>>,
    recursive_roots: &Arc<RwLock<HashSet<PathBuf>>>,
) -> bool {
    if !supported_extension(path) {
        return false;
    }

    if is_vellum_tmp_path(path) {
        return false;
    }

    if path
        .components()
        .any(|component| matches!(component, Component::Normal(name) if name == ".vellum-cache"))
    {
        return false;
    }

    let interesting_match = interesting_paths
        .read()
        .expect("watch interesting paths lock poisoned")
        .contains(path);
    let recursive_match = recursive_roots
        .read()
        .expect("watch recursive roots lock poisoned")
        .iter()
        .any(|root| path.starts_with(root));
    interesting_match || recursive_match
}

fn supported_extension(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    extension.as_deref().is_some_and(|extension| {
        TRACKED_EXTENSIONS
            .iter()
            .any(|tracked| tracked == &extension)
    })
}

fn is_vellum_tmp_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains(".vellum-tmp-"))
}

fn normalize_watch_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| normalize_private_var(path))
}

fn normalize_private_var(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if let Some(stripped) = raw.strip_prefix("/var/") {
        PathBuf::from(format!("/private/var/{stripped}"))
    } else {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::mpsc, time::Duration};

    use super::*;
    use tempfile::TempDir;

    #[test]
    fn watch_emits_event_on_modify() {
        let dir = TempDir::new().unwrap();
        let target = normalize_watch_path(&dir.path().join("note.md"));
        fs::write(&target, b"old").unwrap();
        let (tx, rx) = mpsc::channel();

        let _watch = watch_vault(dir.path(), move |event| {
            tx.send(event).unwrap();
        })
        .unwrap();
        wait_for_watch_registration();
        fs::write(&target, b"new").unwrap();

        let event = receive_event(&rx);
        let expected_hash = *blake3::hash(b"new").as_bytes();
        assert_eq!(
            event,
            WatchEvent::Changed {
                path: target,
                on_disk_hash: expected_hash
            }
        );
    }

    #[test]
    fn watch_filters_non_md_files() {
        let dir = TempDir::new().unwrap();
        let target = normalize_watch_path(&dir.path().join("note.toml"));
        fs::write(&target, b"old").unwrap();
        let (tx, rx) = mpsc::channel();

        let _watch = watch_vault(dir.path(), move |event| {
            tx.send(event).unwrap();
        })
        .unwrap();
        wait_for_watch_registration();
        fs::write(&target, b"new").unwrap();

        assert_no_event(&rx);
    }

    #[test]
    fn watch_accepts_slice3_extensions() {
        let dir = TempDir::new().unwrap();
        let target = normalize_watch_path(&dir.path().join("data.json"));
        fs::write(&target, b"{\"old\":true}").unwrap();
        let (tx, rx) = mpsc::channel();

        let _watch = watch_vault(dir.path(), move |event| {
            tx.send(event).unwrap();
        })
        .unwrap();
        wait_for_watch_registration();
        fs::write(&target, b"{\"new\":true}").unwrap();

        assert!(matches!(receive_event(&rx), WatchEvent::Changed { .. }));
    }

    #[test]
    fn add_path_then_modify_emits_changed_for_arbitrary_location() {
        let canvas_dir = TempDir::new().unwrap();
        let external_dir = TempDir::new().unwrap();
        let target = normalize_watch_path(&external_dir.path().join("external.md"));
        fs::write(&target, b"old").unwrap();
        let (tx, rx) = mpsc::channel();

        let watch = start(move |event| {
            tx.send(event).unwrap();
        })
        .unwrap();
        watch.watch_recursive(canvas_dir.path()).unwrap();
        watch.add_path(&target).unwrap();
        wait_for_watch_registration();
        fs::write(&target, b"new").unwrap();

        let event = receive_event(&rx);
        let expected_hash = *blake3::hash(b"new").as_bytes();
        assert_eq!(
            event,
            WatchEvent::Changed {
                path: target,
                on_disk_hash: expected_hash
            }
        );
    }

    #[test]
    fn remove_path_stops_subsequent_events() {
        let dir = TempDir::new().unwrap();
        let target = normalize_watch_path(&dir.path().join("tracked.md"));
        fs::write(&target, b"old").unwrap();
        let (tx, rx) = mpsc::channel();

        let watch = start(move |event| {
            tx.send(event).unwrap();
        })
        .unwrap();
        watch.add_path(&target).unwrap();
        watch.remove_path(&target).unwrap();
        wait_for_watch_registration();
        fs::write(&target, b"new").unwrap();

        assert_no_event(&rx);
    }

    #[test]
    fn set_paths_replaces_set_atomically() {
        let dir = TempDir::new().unwrap();
        let first = normalize_watch_path(&dir.path().join("first.md"));
        let second = normalize_watch_path(&dir.path().join("second.md"));
        fs::write(&first, b"first-old").unwrap();
        fs::write(&second, b"second-old").unwrap();
        let (tx, rx) = mpsc::channel();

        let watch = start(move |event| {
            tx.send(event).unwrap();
        })
        .unwrap();
        watch.set_paths(vec![first.clone()]).unwrap();
        watch.set_paths(vec![second.clone()]).unwrap();
        wait_for_watch_registration();

        fs::write(&first, b"first-new").unwrap();
        assert_no_event(&rx);

        fs::write(&second, b"second-new").unwrap();
        let event = receive_event(&rx);
        assert!(matches!(
            event,
            WatchEvent::Changed { path, .. } if path == second
        ));
    }

    #[test]
    fn watch_ignores_vellum_tmp_files() {
        let dir = TempDir::new().unwrap();
        let target = normalize_watch_path(&dir.path().join("note.md.vellum-tmp-1234-abcd1234"));
        let (tx, rx) = mpsc::channel();

        let _watch = watch_vault(dir.path(), move |event| {
            tx.send(event).unwrap();
        })
        .unwrap();
        wait_for_watch_registration();
        fs::write(&target, b"tmp").unwrap();

        assert_no_event(&rx);
    }

    #[test]
    fn watch_ignores_vellum_cache() {
        let dir = TempDir::new().unwrap();
        let cache_dir = normalize_watch_path(&dir.path().join(".vellum-cache").join("note.md"));
        fs::create_dir_all(&cache_dir).unwrap();
        let target = cache_dir.join("identity.md");
        let (tx, rx) = mpsc::channel();

        let _watch = watch_vault(dir.path(), move |event| {
            tx.send(event).unwrap();
        })
        .unwrap();
        wait_for_watch_registration();
        fs::write(&target, b"cache").unwrap();

        assert_no_event(&rx);
    }

    fn receive_event(rx: &mpsc::Receiver<WatchEvent>) -> WatchEvent {
        rx.recv_timeout(Duration::from_secs(5))
            .expect("expected watch event")
    }

    fn assert_no_event(rx: &mpsc::Receiver<WatchEvent>) {
        assert!(
            rx.recv_timeout(Duration::from_millis(450)).is_err(),
            "expected no watch event"
        );
    }

    fn wait_for_watch_registration() {
        std::thread::sleep(Duration::from_millis(250));
    }
}
