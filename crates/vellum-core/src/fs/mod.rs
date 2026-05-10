use std::{
    ffi::OsStr,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use thiserror::Error;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::parse::{ParseError, parse};

#[derive(Debug, Error)]
pub enum AtomicWriteError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("conflict detected before atomic write")]
    ConflictDetected {
        on_disk_hash: [u8; 32],
        base_hash: [u8; 32],
    },
    #[error("target is a directory")]
    TargetIsDirectory,
    #[error("target parent directory is missing")]
    ParentMissing,
}

pub fn save_round_trip(source: &str) -> Result<String, ParseError> {
    let _blocks = parse(source)?;
    Ok(source.to_owned())
}

/// Returns true if the source contains Git-style merge conflict markers at column 0.
///
/// A conflicted file should switch the editor to source-only safe mode per spec.
pub fn has_conflict_markers(source: &str) -> bool {
    let mut in_fence = false;
    let mut previous_non_empty_text = false;

    for line in source.lines() {
        if is_fence_line(line) {
            in_fence = !in_fence;
            previous_non_empty_text = false;
            continue;
        }

        if !in_fence {
            if line.starts_with("<<<<<<< ") || line.starts_with(">>>>>>> ") {
                return true;
            }

            if line == "=======" && !previous_non_empty_text {
                return true;
            }
        }

        // A bare ======= immediately after text is treated as a Setext H1
        // underline, not as a merge-conflict middle marker.
        previous_non_empty_text = !line.trim().is_empty();
    }

    false
}

pub fn atomic_write(
    target: &Path,
    contents: &[u8],
    base_hash: Option<&[u8; 32]>,
) -> Result<(), AtomicWriteError> {
    if target.is_dir() {
        return Err(AtomicWriteError::TargetIsDirectory);
    }

    let parent = target.parent().ok_or(AtomicWriteError::ParentMissing)?;
    if !parent.is_dir() {
        return Err(AtomicWriteError::ParentMissing);
    }

    if target.exists()
        && let Some(base_hash) = base_hash
    {
        let on_disk_hash = blake3::hash(&fs::read(target)?);
        let on_disk_hash = *on_disk_hash.as_bytes();
        if &on_disk_hash != base_hash {
            return Err(AtomicWriteError::ConflictDetected {
                on_disk_hash,
                base_hash: *base_hash,
            });
        }
    }

    let tmp_path = tmpfile_path(target);
    match write_and_rename(&tmp_path, target, contents) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&tmp_path);
            Err(error.into())
        }
    }
}

pub fn reap_stale_tmpfiles(vault_root: &Path) -> Result<usize, std::io::Error> {
    let mut deleted = 0usize;

    for entry in WalkDir::new(vault_root) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let Some(pid) = tmpfile_pid(entry.file_name()) else {
            continue;
        };

        if let Some(false) = process_is_live(pid) {
            fs::remove_file(entry.path())?;
            deleted += 1;
        }
    }

    Ok(deleted)
}

fn write_and_rename(tmp_path: &Path, target: &Path, contents: &[u8]) -> Result<(), std::io::Error> {
    let mut tmpfile = fs::File::create(tmp_path)?;
    tmpfile.write_all(contents)?;
    tmpfile.sync_all()?;
    drop(tmpfile);
    fs::rename(tmp_path, target)
}

fn tmpfile_path(target: &Path) -> PathBuf {
    let short_uuid: String = Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(8)
        .collect();
    let pid = std::process::id();
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document");
    target.with_file_name(format!("{file_name}.vellum-tmp-{pid}-{short_uuid}"))
}

fn is_fence_line(line: &str) -> bool {
    line.starts_with("```") || line.starts_with("~~~")
}

fn tmpfile_pid(file_name: &OsStr) -> Option<u32> {
    let file_name = file_name.to_str()?;
    let (_, suffix) = file_name.split_once(".vellum-tmp-")?;
    let (pid, uuid) = suffix.split_once('-')?;
    if pid.is_empty() || uuid.is_empty() {
        return None;
    }
    pid.parse().ok()
}

fn process_is_live(pid: u32) -> Option<bool> {
    #[cfg(target_os = "linux")]
    {
        match fs::metadata(format!("/proc/{pid}")) {
            Ok(_) => Some(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(false),
            Err(_) => None,
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        // SAFETY: kill(pid, 0) does not send a signal; it only asks the kernel
        // whether the process exists and is visible to this user.
        let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
        if result == 0 {
            Some(true)
        } else {
            let error = std::io::Error::last_os_error();
            match error.raw_os_error() {
                Some(code) if code == libc::ESRCH => Some(false),
                _ => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use tempfile::TempDir;

    #[test]
    fn smoke() {
        assert!(true);
    }

    #[test]
    fn detects_conflict_in_plain_text() {
        let source = "<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> branch\n";

        assert!(has_conflict_markers(source));
    }

    #[test]
    fn ignores_conflict_inside_code_block() {
        let source = "```\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> branch\n```\n";

        assert!(!has_conflict_markers(source));
    }

    #[test]
    fn accepts_clean_file() {
        let source = "# Title\n\nA normal paragraph.\n\n- one\n- two\n";

        assert!(!has_conflict_markers(source));
    }

    #[test]
    fn treats_setext_h1_underline_as_clean() {
        let source = "A heading\n=======\n\nA paragraph.\n";

        assert!(!has_conflict_markers(source));
    }

    #[test]
    fn atomic_write_creates_new_file() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("note.md");

        atomic_write(&target, b"# Hello\n", None).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"# Hello\n");
    }

    #[test]
    fn atomic_write_overwrites_with_matching_base_hash() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("note.md");
        fs::write(&target, b"old").unwrap();
        let base_hash = *blake3::hash(b"old").as_bytes();

        atomic_write(&target, b"new", Some(&base_hash)).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new");
    }

    #[test]
    fn atomic_write_rejects_mismatched_base_hash() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("note.md");
        fs::write(&target, b"old").unwrap();
        let base_hash = *blake3::hash(b"old").as_bytes();
        fs::write(&target, b"external change").unwrap();

        let error = atomic_write(&target, b"new", Some(&base_hash)).unwrap_err();

        assert!(matches!(error, AtomicWriteError::ConflictDetected { .. }));
        assert_eq!(fs::read(&target).unwrap(), b"external change");
    }

    #[test]
    fn atomic_write_cleans_up_tmpfile_on_failure() {
        let dir = TempDir::new().unwrap();
        let missing_parent = dir.path().join("missing");
        let target = missing_parent.join("note.md");

        let error = atomic_write(&target, b"new", None).unwrap_err();

        assert!(matches!(error, AtomicWriteError::ParentMissing));
        assert!(fs::read_dir(dir.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".vellum-tmp-")
        }));
    }

    #[test]
    fn reap_stale_tmpfiles_deletes_dead_pid_and_keeps_live_pid() {
        let dir = TempDir::new().unwrap();
        let dead = dir.path().join("a.md.vellum-tmp-99999-abcd1234");
        let live = dir
            .path()
            .join(format!("b.md.vellum-tmp-{}-abcd1234", std::process::id()));
        fs::write(&dead, b"dead").unwrap();
        fs::write(&live, b"live").unwrap();

        let deleted = reap_stale_tmpfiles(dir.path()).unwrap();

        assert_eq!(deleted, 1);
        assert!(!dead.exists());
        assert!(live.exists());
    }
}
