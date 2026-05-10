#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{fs, path::Path};

use vellum_core::{
    block::patch::BlockPatch,
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
            load_sidecar,
            save_sidecar
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Vellum app");
}
