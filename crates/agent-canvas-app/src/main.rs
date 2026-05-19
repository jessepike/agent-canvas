#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{fs, path::Path};

use vellum_core::{
    block::patch::BlockPatch,
    fs::{AtomicWriteError, OpenDocument, WriteResult, atomic_write, has_conflict_markers},
    sidecar::{self, IdentityMap},
};

#[tauri::command]
fn parse_document(source: String) -> Result<Vec<vellum_core::parse::Block>, String> {
    vellum_core::parse::parse(&source).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_document(source: String, patches: Vec<BlockPatch>) -> Result<String, String> {
    vellum_core::save(&source, &patches).map_err(|error| error.to_string())
}

#[tauri::command]
fn open_document(doc_path: String) -> Result<OpenDocument, String> {
    let doc_path = absolute_doc_path(&doc_path)?;
    ensure_regular_file(doc_path)?;

    let bytes = fs::read(doc_path).map_err(|error| error.to_string())?;
    let base_hash = *vellum_core::hash::content_hash(&bytes).as_bytes();
    let source = String::from_utf8(bytes).map_err(|error| error.to_string())?;

    Ok(OpenDocument {
        path: doc_path.to_string_lossy().into_owned(),
        has_conflict_markers: has_conflict_markers(&source),
        source,
        base_hash,
    })
}

#[tauri::command]
fn write_document(
    doc_path: String,
    source: String,
    base_hash: [u8; 32],
) -> Result<WriteResult, String> {
    let doc_path = absolute_doc_path(&doc_path)?;

    match atomic_write(doc_path, source.as_bytes(), Some(&base_hash)) {
        Ok(new_hash) => Ok(WriteResult { new_hash }),
        // UI pattern-matches this string prefix until the 30B-05 typed
        // three-way merge error channel exists.
        Err(AtomicWriteError::ConflictDetected { .. }) => {
            Err("CONFLICT: file changed on disk before save".to_owned())
        }
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
fn load_sidecar(doc_path: String) -> Result<IdentityMap, String> {
    let doc_path = absolute_doc_path(&doc_path)?;
    let vault_root = vault_root_for_absolute_doc(doc_path)?;
    let doc_source = fs::read_to_string(doc_path).map_err(|error| error.to_string())?;

    // Gate 30B IPC uses an absolute doc path and treats the document parent as
    // the temporary vault root. Vault state can replace this once open-vault
    // app state exists.
    let migrated = sidecar::load_or_migrate(vault_root, doc_path, &doc_source)
        .map_err(|error| error.to_string())?;
    Ok(migrated.unwrap_or_else(|| IdentityMap {
        source_hash: *vellum_core::hash::content_hash(doc_source.as_bytes()).as_bytes(),
        block_ids: Vec::new(),
    }))
}

#[tauri::command]
fn save_sidecar(doc_path: String, map: IdentityMap) -> Result<(), String> {
    let doc_path = absolute_doc_path(&doc_path)?;
    let vault_root = vault_root_for_absolute_doc(doc_path)?;

    // See load_sidecar: this command intentionally keeps the same temporary
    // absolute-path convention until vault-root app state lands.
    sidecar::save(vault_root, doc_path, &map).map_err(|error| error.to_string())
}

fn absolute_doc_path(doc_path: &str) -> Result<&Path, String> {
    let path = Path::new(doc_path);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err("doc_path must be absolute until vault-root app state lands".to_owned())
    }
}

fn ensure_regular_file(doc_path: &Path) -> Result<(), String> {
    let metadata = fs::metadata(doc_path).map_err(|error| error.to_string())?;
    if metadata.is_file() {
        Ok(())
    } else {
        Err("doc_path must point to a regular file".to_owned())
    }
}

fn vault_root_for_absolute_doc(doc_path: &Path) -> Result<&Path, String> {
    doc_path
        .parent()
        .ok_or_else(|| "doc_path must have a parent directory".to_owned())
}

fn main() {
    tauri::Builder::<tauri::Wry>::default()
        .invoke_handler(tauri::generate_handler![
            parse_document,
            save_document,
            open_document,
            write_document,
            load_sidecar,
            save_sidecar
        ])
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("failed to run AgentCanvas app");
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn open_document_reads_source_hash_and_conflict_marker_state() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("note.md");
        let source = "<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> branch\n";
        fs::write(&target, source).unwrap();

        let opened = open_document(target.to_string_lossy().into_owned()).unwrap();

        assert_eq!(opened.path, target.to_string_lossy());
        assert_eq!(opened.source, source);
        assert_eq!(
            opened.base_hash,
            *vellum_core::hash::content_hash(source.as_bytes()).as_bytes()
        );
        assert!(opened.has_conflict_markers);
    }

    #[test]
    fn open_document_rejects_missing_or_non_file_paths() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("missing.md");

        assert!(open_document(missing.to_string_lossy().into_owned()).is_err());
        assert!(open_document(dir.path().to_string_lossy().into_owned()).is_err());
    }

    #[test]
    fn write_document_round_trips_and_returns_new_hash() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("note.md");
        fs::write(&target, b"old").unwrap();
        let base_hash = *vellum_core::hash::content_hash(b"old").as_bytes();

        let result = write_document(
            target.to_string_lossy().into_owned(),
            "new".to_owned(),
            base_hash,
        )
        .unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(
            result.new_hash,
            *vellum_core::hash::content_hash(b"new").as_bytes()
        );
    }

    #[test]
    fn write_document_reports_conflict_with_typed_prefix() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("note.md");
        fs::write(&target, b"base").unwrap();
        let base_hash = *vellum_core::hash::content_hash(b"base").as_bytes();
        fs::write(&target, b"external").unwrap();

        let error = write_document(
            target.to_string_lossy().into_owned(),
            "ours".to_owned(),
            base_hash,
        )
        .unwrap_err();

        assert!(error.starts_with("CONFLICT:"));
        assert_eq!(fs::read(&target).unwrap(), b"external");
    }
}
