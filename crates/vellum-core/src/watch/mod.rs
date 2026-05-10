use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
    sync::mpsc,
    thread::{self, JoinHandle},
    time::Duration,
};

use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{ModifyKind, RenameMode},
};
use thiserror::Error;

const MODIFY_DEBOUNCE: Duration = Duration::from_millis(50);

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
    watcher: Option<RecommendedWatcher>,
    worker: Option<JoinHandle<()>>,
}

pub fn watch_vault(
    vault_root: &Path,
    callback: impl Fn(WatchEvent) + Send + 'static,
) -> Result<WatchHandle, WatchError> {
    let vault_root = vault_root.to_path_buf();
    let (event_tx, event_rx) = mpsc::channel();

    let mut watcher = notify::recommended_watcher(move |result| {
        if event_tx.send(result).is_err() {
            tracing::warn!("watch event received after worker stopped");
        }
    })?;

    watcher.watch(&vault_root, RecursiveMode::Recursive)?;

    let worker = thread::spawn(move || {
        run_watch_worker(vault_root, event_rx, callback);
    });

    Ok(WatchHandle {
        watcher: Some(watcher),
        worker: Some(worker),
    })
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        drop(self.watcher.take());
        if let Some(worker) = self.worker.take()
            && let Err(error) = worker.join()
        {
            tracing::warn!(?error, "watch worker thread panicked");
        }
    }
}

fn run_watch_worker(
    vault_root: PathBuf,
    event_rx: mpsc::Receiver<notify::Result<Event>>,
    callback: impl Fn(WatchEvent),
) {
    let mut pending_changed = HashSet::new();

    loop {
        match event_rx.recv_timeout(MODIFY_DEBOUNCE) {
            Ok(Ok(event)) => {
                process_notify_event(&vault_root, event, &callback, &mut pending_changed);
            }
            Ok(Err(error)) => {
                tracing::warn!(?error, "file watch backend error");
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                flush_changed(&mut pending_changed, &callback);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                flush_changed(&mut pending_changed, &callback);
                break;
            }
        }
    }
}

fn process_notify_event(
    vault_root: &Path,
    event: Event,
    callback: &impl Fn(WatchEvent),
    pending_changed: &mut HashSet<PathBuf>,
) {
    match event.kind {
        EventKind::Create(_) => {
            for path in filter_paths(vault_root, event.paths) {
                callback(WatchEvent::Created { path });
            }
        }
        EventKind::Remove(_) => {
            for path in filter_paths(vault_root, event.paths) {
                callback(WatchEvent::Removed { path });
            }
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() >= 2 => {
            let from = event.paths[0].clone();
            let to = event.paths[1].clone();
            if should_watch_path(vault_root, &from) && should_watch_path(vault_root, &to) {
                callback(WatchEvent::Renamed { from, to });
            }
        }
        EventKind::Modify(ModifyKind::Data(_)) | EventKind::Modify(ModifyKind::Any) => {
            // Backends and editors often emit several modify notifications for
            // one user-visible write. Debounce at the worker boundary so the UI
            // sees one Changed event per path after the write settles.
            pending_changed.extend(filter_paths(vault_root, event.paths));
        }
        _ => {}
    }
}

fn flush_changed(pending_changed: &mut HashSet<PathBuf>, callback: &impl Fn(WatchEvent)) {
    for path in pending_changed.drain() {
        match fs::read(&path) {
            Ok(contents) => {
                let on_disk_hash = *blake3::hash(&contents).as_bytes();
                callback(WatchEvent::Changed { path, on_disk_hash });
            }
            Err(error) => {
                tracing::warn!(?path, ?error, "could not hash changed file");
            }
        }
    }
}

fn filter_paths(vault_root: &Path, paths: Vec<PathBuf>) -> impl Iterator<Item = PathBuf> + '_ {
    paths
        .into_iter()
        .filter(move |path| should_watch_path(vault_root, path))
}

fn should_watch_path(vault_root: &Path, path: &Path) -> bool {
    if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
        return false;
    }

    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains(".vellum-tmp-"))
    {
        return false;
    }

    let relative = path.strip_prefix(vault_root).unwrap_or(path);
    !relative
        .components()
        .any(|component| matches!(component, Component::Normal(name) if name == ".vellum-cache"))
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::mpsc, time::Duration};

    use super::*;
    use tempfile::TempDir;

    #[test]
    fn watch_emits_event_on_modify() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("note.md");
        fs::write(&target, b"old").unwrap();
        let (tx, rx) = mpsc::channel();

        let _watch = watch_vault(dir.path(), move |event| {
            tx.send(event).unwrap();
        })
        .unwrap();
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
        let target = dir.path().join("note.txt");
        fs::write(&target, b"old").unwrap();
        let (tx, rx) = mpsc::channel();

        let _watch = watch_vault(dir.path(), move |event| {
            tx.send(event).unwrap();
        })
        .unwrap();
        fs::write(&target, b"new").unwrap();

        assert_no_event(&rx);
    }

    #[test]
    fn watch_ignores_vellum_tmp_files() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("note.md.vellum-tmp-1234-abcd1234");
        let (tx, rx) = mpsc::channel();

        let _watch = watch_vault(dir.path(), move |event| {
            tx.send(event).unwrap();
        })
        .unwrap();
        fs::write(&target, b"tmp").unwrap();

        assert_no_event(&rx);
    }

    #[test]
    fn watch_ignores_vellum_cache() {
        let dir = TempDir::new().unwrap();
        let cache_dir = dir.path().join(".vellum-cache").join("note.md");
        fs::create_dir_all(&cache_dir).unwrap();
        let target = cache_dir.join("identity.md");
        let (tx, rx) = mpsc::channel();

        let _watch = watch_vault(dir.path(), move |event| {
            tx.send(event).unwrap();
        })
        .unwrap();
        fs::write(&target, b"cache").unwrap();

        assert_no_event(&rx);
    }

    fn receive_event(rx: &mpsc::Receiver<WatchEvent>) -> WatchEvent {
        rx.recv_timeout(Duration::from_secs(1))
            .expect("expected watch event")
    }

    fn assert_no_event(rx: &mpsc::Receiver<WatchEvent>) {
        assert!(
            rx.recv_timeout(Duration::from_millis(250)).is_err(),
            "expected no watch event"
        );
    }
}
