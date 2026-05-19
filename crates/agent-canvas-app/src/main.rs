#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::UNIX_EPOCH,
};

#[cfg(target_os = "macos")]
use std::{
    io::Write,
    process::{Command, Stdio},
};

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use vellum_core::{
    block::patch::BlockPatch,
    fs::{AtomicWriteError, OpenDocument, WriteResult, atomic_write, has_conflict_markers},
    sidecar::{self, IdentityMap},
    watch::{self, WatchEvent, WatchHandle},
};
use walkdir::WalkDir;

struct AppState {
    paths: AgentCanvasPaths,
    db: Mutex<Connection>,
    watcher: Mutex<Option<WatchHandle>>,
}

#[derive(Debug, Clone)]
struct AgentCanvasPaths {
    cloud_docs_root: PathBuf,
    canvas_root: PathBuf,
    user_symlink: PathBuf,
    inbox_dir: PathBuf,
    projects_dir: PathBuf,
    archive_dir: PathBuf,
    state_db: PathBuf,
    persona_registry: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct BootstrapInfo {
    canvas_root: String,
    inbox_dir: String,
    projects_dir: String,
    archive_dir: String,
    state_db: String,
    user_path: String,
}

#[derive(Debug, Clone, Serialize)]
struct FileMetadata {
    path: String,
    relative_path: String,
    name: String,
    extension: String,
    size: u64,
    mtime: i64,
    last_seen_hash: [u8; 32],
    pinned: bool,
    archived: bool,
    last_read_at: Option<i64>,
    persona: String,
}

#[derive(Debug, Clone, Serialize)]
struct Persona {
    name: String,
    color: String,
    display_label: String,
    source: String,
}

#[derive(Debug, Clone, Serialize)]
struct PersonaRegistry {
    personas: Vec<Persona>,
    warning: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SendPayload {
    path: String,
    project: String,
    persona: String,
    contents: String,
    note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct FsEventPayload {
    kind: &'static str,
    path: Option<String>,
}

#[tauri::command]
fn bootstrap_info(state: tauri::State<AppState>) -> BootstrapInfo {
    state.paths.bootstrap_info()
}

#[tauri::command]
fn list_inbox(state: tauri::State<AppState>) -> Result<Vec<FileMetadata>, String> {
    list_files_under(&state.paths.inbox_dir, &state.paths.canvas_root, &state.db)
}

#[tauri::command]
fn list_project_files(
    state: tauri::State<AppState>,
    project: String,
) -> Result<Vec<FileMetadata>, String> {
    let project_dir = state
        .paths
        .projects_dir
        .join(safe_project_segment(&project)?);
    list_files_under(&project_dir, &state.paths.canvas_root, &state.db)
}

#[tauri::command]
fn list_projects(state: tauri::State<AppState>) -> Result<Vec<String>, String> {
    let mut projects = Vec::new();
    for entry in fs::read_dir(&state.paths.projects_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
            && let Some(name) = entry.file_name().to_str()
        {
            projects.push(name.to_owned());
        }
    }
    projects.sort();
    Ok(projects)
}

#[tauri::command]
fn list_personas(state: tauri::State<AppState>) -> Result<PersonaRegistry, String> {
    resolve_personas(&state.paths.persona_registry, &state.db)
}

#[tauri::command]
fn send_to_clipboard(payload: SendPayload) -> Result<String, String> {
    let formatted = format_send_payload(&payload);
    write_clipboard(&formatted)?;
    Ok(formatted)
}

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

    sidecar::save(vault_root, doc_path, &map).map_err(|error| error.to_string())
}

fn bootstrap() -> Result<AppState, String> {
    let paths = AgentCanvasPaths::resolve()?;
    paths.ensure()?;
    let db = open_state_db(&paths.state_db)?;
    Ok(AppState {
        paths,
        db: Mutex::new(db),
        watcher: Mutex::new(None),
    })
}

impl AgentCanvasPaths {
    fn resolve() -> Result<Self, String> {
        let home = home_dir()?;
        let cloud_docs_root = home
            .join("Library")
            .join("Mobile Documents")
            .join("com~apple~CloudDocs");
        let canvas_root = cloud_docs_root.join("AgentCanvas");
        let user_symlink = home.join("iCloud");
        let app_support = home
            .join("Library")
            .join("Application Support")
            .join("AgentCanvas");
        let persona_registry = std::env::var_os("AGENTCANVAS_PERSONA_REGISTRY")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                home.join("code")
                    .join("_shared")
                    .join("pike-agents")
                    .join("plugins")
            });

        Ok(Self {
            cloud_docs_root,
            inbox_dir: canvas_root.join("Inbox"),
            projects_dir: canvas_root.join("Projects"),
            archive_dir: canvas_root.join("Archive"),
            canvas_root,
            user_symlink,
            state_db: app_support.join("state.db"),
            persona_registry,
        })
    }

    fn ensure(&self) -> Result<(), String> {
        fs::create_dir_all(self.inbox_dir.join("captures")).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.projects_dir.join("Default")).map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.archive_dir).map_err(|error| error.to_string())?;

        if !self.user_symlink.exists() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(&self.cloud_docs_root, &self.user_symlink)
                .map_err(|error| error.to_string())?;
        }

        if let Some(parent) = self.state_db.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn bootstrap_info(&self) -> BootstrapInfo {
        BootstrapInfo {
            canvas_root: self.canvas_root.to_string_lossy().into_owned(),
            inbox_dir: self.inbox_dir.to_string_lossy().into_owned(),
            projects_dir: self.projects_dir.to_string_lossy().into_owned(),
            archive_dir: self.archive_dir.to_string_lossy().into_owned(),
            state_db: self.state_db.to_string_lossy().into_owned(),
            user_path: self
                .user_symlink
                .join("AgentCanvas")
                .to_string_lossy()
                .into_owned(),
        }
    }
}

fn open_state_db(path: &Path) -> Result<Connection, String> {
    let db = Connection::open(path).map_err(|error| error.to_string())?;
    db.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        CREATE TABLE IF NOT EXISTS files (
          path TEXT PRIMARY KEY,
          last_seen_hash BLOB NOT NULL,
          size INTEGER,
          mtime INTEGER,
          pinned INTEGER DEFAULT 0,
          archived INTEGER DEFAULT 0,
          last_read_at INTEGER
        );
        CREATE TABLE IF NOT EXISTS agent_sessions (
          id TEXT PRIMARY KEY,
          persona TEXT NOT NULL,
          backbone TEXT NOT NULL,
          context TEXT,
          connected_at INTEGER NOT NULL,
          last_active INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS comments (
          id TEXT PRIMARY KEY,
          file_path TEXT NOT NULL,
          anchor_text TEXT,
          anchor_offset INTEGER,
          author TEXT,
          body TEXT,
          thread_id TEXT,
          resolved INTEGER DEFAULT 0,
          created_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS pending_edits (
          id TEXT PRIMARY KEY,
          file_path TEXT NOT NULL,
          proposer TEXT,
          diff TEXT,
          reasoning TEXT,
          created_at INTEGER NOT NULL,
          status TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS personas (
          name TEXT PRIMARY KEY,
          color TEXT NOT NULL,
          display_label TEXT NOT NULL,
          source TEXT NOT NULL,
          updated_at INTEGER NOT NULL
        );
        "#,
    )
    .map_err(|error| error.to_string())?;
    Ok(db)
}

fn list_files_under(
    root: &Path,
    canvas_root: &Path,
    db: &Mutex<Connection>,
) -> Result<Vec<FileMetadata>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let conn = db.lock().map_err(|_| "state db lock poisoned".to_owned())?;

    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() || !is_supported_artifact(entry.path()) {
            continue;
        }
        let file = metadata_for_file(entry.path(), canvas_root)?;
        upsert_file_state(&conn, &file)?;
        files.push(file);
    }

    files.sort_by(|left, right| {
        right
            .mtime
            .cmp(&left.mtime)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(files)
}

fn metadata_for_file(path: &Path, canvas_root: &Path) -> Result<FileMetadata, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let relative_path = path.strip_prefix(canvas_root).unwrap_or(path);

    Ok(FileMetadata {
        path: path.to_string_lossy().into_owned(),
        relative_path: relative_path.to_string_lossy().into_owned(),
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("artifact")
            .to_owned(),
        extension: path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase(),
        size: metadata.len(),
        mtime,
        last_seen_hash: *vellum_core::hash::content_hash(&bytes).as_bytes(),
        pinned: false,
        archived: false,
        last_read_at: None,
        persona: infer_persona(path),
    })
}

fn upsert_file_state(conn: &Connection, file: &FileMetadata) -> Result<(), String> {
    let existing_path: Option<String> = conn
        .query_row(
            "SELECT path FROM files WHERE last_seen_hash = ?1 AND path != ?2 LIMIT 1",
            params![file.last_seen_hash.as_slice(), file.path],
            |row| row.get(0),
        )
        .ok();

    if let Some(existing_path) = existing_path {
        conn.execute(
            "UPDATE files SET path = ?1, size = ?2, mtime = ?3 WHERE path = ?4",
            params![file.path, file.size as i64, file.mtime, existing_path],
        )
        .map_err(|error| error.to_string())?;
    }

    conn.execute(
        r#"
        INSERT INTO files(path, last_seen_hash, size, mtime, pinned, archived)
        VALUES (?1, ?2, ?3, ?4, 0, 0)
        ON CONFLICT(path) DO UPDATE SET
          last_seen_hash = excluded.last_seen_hash,
          size = excluded.size,
          mtime = excluded.mtime
        "#,
        params![
            file.path,
            file.last_seen_hash.as_slice(),
            file.size as i64,
            file.mtime
        ],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn is_supported_artifact(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("md" | "markdown" | "html" | "htm")
    )
}

fn resolve_personas(
    registry_root: &Path,
    db: &Mutex<Connection>,
) -> Result<PersonaRegistry, String> {
    let mut personas = Vec::new();
    let mut warning = None;

    if registry_root.exists() {
        for &(name, fallback_color) in builtin_persona_colors() {
            let path = registry_root
                .join(name)
                .join("agents")
                .join(format!("{name}.md"));
            if let Ok(source) = fs::read_to_string(&path) {
                let color = frontmatter_value(&source, "color")
                    .unwrap_or_else(|| fallback_color.to_owned());
                personas.push(Persona {
                    name: name.to_owned(),
                    color,
                    display_label: display_label(name),
                    source: "pike-agents".to_owned(),
                });
            }
        }
        if personas.is_empty() {
            warning = Some("persona registry unavailable, using defaults".to_owned());
            personas = builtin_personas();
        }
    } else {
        warning = Some("persona registry unavailable, using defaults".to_owned());
        personas = builtin_personas();
    }

    let conn = db.lock().map_err(|_| "state db lock poisoned".to_owned())?;
    for persona in &personas {
        conn.execute(
            r#"
            INSERT INTO personas(name, color, display_label, source, updated_at)
            VALUES (?1, ?2, ?3, ?4, strftime('%s','now'))
            ON CONFLICT(name) DO UPDATE SET
              color = excluded.color,
              display_label = excluded.display_label,
              source = excluded.source,
              updated_at = excluded.updated_at
            "#,
            params![
                persona.name,
                persona.color,
                persona.display_label,
                persona.source
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    Ok(PersonaRegistry { personas, warning })
}

fn frontmatter_value(source: &str, key: &str) -> Option<String> {
    let mut lines = source.lines();
    if lines.next()? != "---" {
        return None;
    }

    for line in lines {
        if line == "---" {
            break;
        }
        if let Some((candidate, value)) = line.split_once(':')
            && candidate.trim() == key
        {
            return Some(value.trim().trim_matches('"').to_owned());
        }
    }
    None
}

fn infer_persona(path: &Path) -> String {
    let lower = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    builtin_persona_colors()
        .iter()
        .find_map(|(name, _)| lower.contains(name).then(|| (*name).to_owned()))
        .unwrap_or_else(|| "claude".to_owned())
}

fn builtin_personas() -> Vec<Persona> {
    builtin_persona_colors()
        .iter()
        .map(|(name, color)| Persona {
            name: (*name).to_owned(),
            color: (*color).to_owned(),
            display_label: display_label(name),
            source: "built-in".to_owned(),
        })
        .collect()
}

fn builtin_persona_colors() -> &'static [(&'static str, &'static str)] {
    &[
        ("cpo", "blue"),
        ("cto", "indigo"),
        ("cfo", "green"),
        ("cro", "orange"),
        ("cmo", "purple"),
        ("ciso", "red"),
        ("krypton", "magenta"),
        ("forge", "amber"),
        ("agf-architect", "teal"),
        ("claude", "neutral"),
        ("codex", "neutral"),
    ]
}

fn display_label(name: &str) -> String {
    if name == "agf-architect" {
        "AGF Architect".to_owned()
    } else {
        name.to_ascii_uppercase()
    }
}

fn format_send_payload(payload: &SendPayload) -> String {
    let note = payload.note.as_deref().unwrap_or("").trim();
    let note = if note.is_empty() { "" } else { note };
    format!(
        "Path: {}\nProject: {}\nPersona inferred: {}\n\n{}\n\n-- Jesse's note: {}",
        payload.path, payload.project, payload.persona, payload.contents, note
    )
}

#[cfg(target_os = "macos")]
fn write_clipboard(contents: &str) -> Result<(), String> {
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| "pbcopy stdin unavailable".to_owned())?
        .write_all(contents.as_bytes())
        .map_err(|error| error.to_string())?;
    let status = child.wait().map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("pbcopy failed".to_owned())
    }
}

#[cfg(not(target_os = "macos"))]
fn write_clipboard(contents: &str) -> Result<(), String> {
    fs::write("/tmp/agentcanvas-clipboard.txt", contents).map_err(|error| error.to_string())
}

fn safe_project_segment(project: &str) -> Result<&str, String> {
    if project.is_empty()
        || project.contains('/')
        || project.contains('\\')
        || project == "."
        || project == ".."
    {
        Err("invalid project name".to_owned())
    } else {
        Ok(project)
    }
}

fn absolute_doc_path(doc_path: &str) -> Result<&Path, String> {
    let path = Path::new(doc_path);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err("doc_path must be absolute".to_owned())
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

fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".to_owned())
}

fn main() {
    let app_state = bootstrap().expect("failed to bootstrap AgentCanvas");

    tauri::Builder::<tauri::Wry>::default()
        .manage(app_state)
        .setup(|app| {
            let state = app.state::<AppState>();
            let canvas_root = state.paths.canvas_root.clone();
            let app_handle = app.handle().clone();
            let watcher = watch::watch_vault(&canvas_root, move |event| {
                let payload = fs_event_payload(event);
                let _ = app_handle.emit("agentcanvas://fs-event", payload);
            })?;
            *state.watcher.lock().map_err(|_| "watcher lock poisoned")? = Some(watcher);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap_info,
            list_inbox,
            list_projects,
            list_project_files,
            list_personas,
            send_to_clipboard,
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

fn fs_event_payload(event: WatchEvent) -> FsEventPayload {
    match event {
        WatchEvent::Changed { path, .. } => FsEventPayload {
            kind: "changed",
            path: Some(path.to_string_lossy().into_owned()),
        },
        WatchEvent::Created { path } => FsEventPayload {
            kind: "created",
            path: Some(path.to_string_lossy().into_owned()),
        },
        WatchEvent::Removed { path } => FsEventPayload {
            kind: "removed",
            path: Some(path.to_string_lossy().into_owned()),
        },
        WatchEvent::Renamed { to, .. } => FsEventPayload {
            kind: "renamed",
            path: Some(to.to_string_lossy().into_owned()),
        },
    }
}
