#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
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
    contents: String,
    note: Option<String>,
    action_verb: String,
}

#[derive(Debug, Clone, Serialize)]
struct AgentSession {
    id: String,
    persona: String,
    backbone: String,
    context: String,
    connected_at: i64,
    last_active: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct AddAgentSessionInput {
    persona: String,
    backbone: String,
    context: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ConflictStrategy {
    Replace,
    KeepBoth,
    Cancel,
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
            upsert_project(&state.db, name, None)?;
            projects.push(name.to_owned());
        }
    }
    projects.sort();
    Ok(projects)
}

#[tauri::command]
fn get_project_default_agent(
    state: tauri::State<AppState>,
    project: String,
) -> Result<Option<String>, String> {
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    let default_agent_session_id = conn
        .query_row(
            "SELECT default_agent_session_id FROM projects WHERE name = ?1",
            params![project],
            |row| row.get(0),
        )
        .ok();
    Ok(default_agent_session_id)
}

#[tauri::command]
fn set_project_default_agent(
    state: tauri::State<AppState>,
    project: String,
    session_id: String,
) -> Result<(), String> {
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    let session_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM agent_sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if session_exists == 0 {
        return Err("agent session not found".to_owned());
    }
    conn.execute(
        r#"
        INSERT INTO projects(name, default_agent_session_id, updated_at)
        VALUES (?1, ?2, strftime('%s','now'))
        ON CONFLICT(name) DO UPDATE SET
          default_agent_session_id = excluded.default_agent_session_id,
          updated_at = excluded.updated_at
        "#,
        params![project, session_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn list_personas(state: tauri::State<AppState>) -> Result<PersonaRegistry, String> {
    resolve_personas(&state.paths.persona_registry, &state.db)
}

#[tauri::command]
fn get_default_action_verb(state: tauri::State<AppState>) -> Result<String, String> {
    get_setting(&state.db, "default_action_verb")
        .map(|value| value.unwrap_or_else(|| "Review".to_owned()))
}

#[tauri::command]
fn set_default_action_verb(state: tauri::State<AppState>, verb: String) -> Result<(), String> {
    let verb = verb.trim();
    if verb.is_empty() {
        return Err("action verb cannot be empty".to_owned());
    }
    set_setting(&state.db, "default_action_verb", verb)
}

#[tauri::command]
fn toggle_pin(state: tauri::State<AppState>, path: String) -> Result<bool, String> {
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    let current: i64 = conn
        .query_row(
            "SELECT pinned FROM files WHERE path = ?1",
            params![path],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let next = if current == 0 { 1 } else { 0 };
    conn.execute(
        "UPDATE files SET pinned = ?1 WHERE path = ?2",
        params![next, path],
    )
    .map_err(|error| error.to_string())?;
    Ok(next == 1)
}

#[tauri::command]
fn archive_file(state: tauri::State<AppState>, path: String) -> Result<String, String> {
    let source = absolute_doc_path(&path)?;
    ensure_regular_file(source)?;
    let file_name = source
        .file_name()
        .ok_or_else(|| "archive source has no filename".to_owned())?;
    let target = unique_archive_path(&state.paths.archive_dir.join(file_name));
    fs::rename(source, &target).map_err(|error| error.to_string())?;
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    conn.execute(
        "UPDATE files SET path = ?1, archived = 1 WHERE path = ?2",
        params![target.to_string_lossy(), path],
    )
    .map_err(|error| error.to_string())?;
    Ok(target.to_string_lossy().into_owned())
}

#[tauri::command]
fn copy_paths_to_inbox(
    state: tauri::State<AppState>,
    paths: Vec<String>,
) -> Result<Vec<FileMetadata>, String> {
    let mut copied = Vec::new();
    for path in paths {
        let source = PathBuf::from(path);
        ensure_regular_file(&source)?;
        let file_name = source
            .file_name()
            .ok_or_else(|| "dropped file has no filename".to_owned())?;
        let target = unique_path(&state.paths.inbox_dir.join(file_name));
        fs::copy(&source, &target).map_err(|error| error.to_string())?;
        let file = metadata_for_file(&target, &state.paths.canvas_root)?;
        let conn = state
            .db
            .lock()
            .map_err(|_| "state db lock poisoned".to_owned())?;
        upsert_file_state(&conn, &file)?;
        copied.push(file);
    }
    Ok(copied)
}

#[tauri::command]
fn move_file_to_project(
    state: tauri::State<AppState>,
    path: String,
    project: String,
    strategy: ConflictStrategy,
) -> Result<FileMetadata, String> {
    let project = safe_project_segment(&project)?;
    move_file_to_target(
        &state,
        &path,
        &state.paths.projects_dir.join(project),
        false,
        strategy,
    )
}

#[tauri::command]
fn move_file_to_archive(
    state: tauri::State<AppState>,
    path: String,
    strategy: ConflictStrategy,
) -> Result<FileMetadata, String> {
    move_file_to_target(&state, &path, &state.paths.archive_dir, true, strategy)
}

#[tauri::command]
fn target_file_exists(
    state: tauri::State<AppState>,
    target: String,
    project: Option<String>,
    filename: String,
) -> Result<bool, String> {
    if filename.contains('/') || filename.contains('\\') || filename.is_empty() {
        return Err("invalid filename".to_owned());
    }
    let dir = match target.as_str() {
        "archive" => state.paths.archive_dir.clone(),
        "project" => {
            let project = project.ok_or_else(|| "project is required".to_owned())?;
            state
                .paths
                .projects_dir
                .join(safe_project_segment(&project)?)
        }
        _ => return Err("invalid target".to_owned()),
    };
    Ok(dir.join(filename).exists())
}

#[tauri::command]
fn copy_text_to_clipboard(text: String) -> Result<String, String> {
    write_clipboard(&text)?;
    Ok(text)
}

#[tauri::command]
fn reveal_in_finder(path: String) -> Result<(), String> {
    let path = absolute_doc_path(&path)?;
    ensure_regular_file(path)?;
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .arg("-R")
            .arg(path)
            .status()
            .map_err(|error| error.to_string())?;
        if status.success() {
            return Ok(());
        }
        return Err("open -R failed".to_owned());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err("Reveal in Finder is only available on macOS".to_owned())
    }
}

#[tauri::command]
fn delete_file(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    let source = absolute_doc_path(&path)?;
    ensure_regular_file(source)?;
    ensure_under_root(source, &state.paths.canvas_root)?;
    fs::remove_file(source).map_err(|error| error.to_string())?;
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    conn.execute("DELETE FROM files WHERE path = ?1", params![path])
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn send_to_clipboard(
    state: tauri::State<AppState>,
    payload: SendPayload,
) -> Result<String, String> {
    let formatted = format_send_payload(&payload, &state.paths.canvas_root)?;
    write_clipboard(&formatted)?;
    Ok(formatted)
}

#[tauri::command]
fn list_agent_sessions(state: tauri::State<AppState>) -> Result<Vec<AgentSession>, String> {
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    let mut statement = conn
        .prepare(
            "SELECT id, persona, backbone, COALESCE(context, ''), connected_at, last_active
             FROM agent_sessions ORDER BY last_active DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(AgentSession {
                id: row.get(0)?,
                persona: row.get(1)?,
                backbone: row.get(2)?,
                context: row.get(3)?,
                connected_at: row.get(4)?,
                last_active: row.get(5)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn add_agent_session(
    state: tauri::State<AppState>,
    input: AddAgentSessionInput,
) -> Result<AgentSession, String> {
    let now = unix_now();
    let session = AgentSession {
        id: uuid::Uuid::new_v4().to_string(),
        persona: input.persona,
        backbone: input.backbone,
        context: input.context,
        connected_at: now,
        last_active: now,
    };
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    conn.execute(
        "INSERT INTO agent_sessions(id, persona, backbone, context, connected_at, last_active)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            session.id,
            session.persona,
            session.backbone,
            session.context,
            session.connected_at,
            session.last_active
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(session)
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
        CREATE TABLE IF NOT EXISTS settings (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL,
          updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS projects (
          name TEXT PRIMARY KEY,
          default_agent_session_id TEXT REFERENCES agent_sessions(id) ON DELETE SET NULL,
          updated_at INTEGER NOT NULL
        );
        "#,
    )
    .map_err(|error| error.to_string())?;
    Ok(db)
}

fn get_setting(db: &Mutex<Connection>, key: &str) -> Result<Option<String>, String> {
    let conn = db.lock().map_err(|_| "state db lock poisoned".to_owned())?;
    let value = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .ok();
    Ok(value)
}

fn set_setting(db: &Mutex<Connection>, key: &str, value: &str) -> Result<(), String> {
    let conn = db.lock().map_err(|_| "state db lock poisoned".to_owned())?;
    conn.execute(
        r#"
        INSERT INTO settings(key, value, updated_at)
        VALUES (?1, ?2, strftime('%s','now'))
        ON CONFLICT(key) DO UPDATE SET
          value = excluded.value,
          updated_at = excluded.updated_at
        "#,
        params![key, value],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn upsert_project(
    db: &Mutex<Connection>,
    name: &str,
    default_agent_session_id: Option<&str>,
) -> Result<(), String> {
    let conn = db.lock().map_err(|_| "state db lock poisoned".to_owned())?;
    conn.execute(
        r#"
        INSERT INTO projects(name, default_agent_session_id, updated_at)
        VALUES (?1, ?2, strftime('%s','now'))
        ON CONFLICT(name) DO UPDATE SET
          updated_at = excluded.updated_at
        "#,
        params![name, default_agent_session_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
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
        let mut file = metadata_for_file(entry.path(), canvas_root)?;
        upsert_file_state(&conn, &file)?;
        hydrate_file_state(&conn, &mut file)?;
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

fn hydrate_file_state(conn: &Connection, file: &mut FileMetadata) -> Result<(), String> {
    let state: Option<(i64, i64, Option<i64>)> = conn
        .query_row(
            "SELECT pinned, archived, last_read_at FROM files WHERE path = ?1",
            params![file.path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .ok();
    if let Some((pinned, archived, last_read_at)) = state {
        file.pinned = pinned != 0;
        file.archived = archived != 0;
        file.last_read_at = last_read_at;
    }
    Ok(())
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
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| !name.starts_with('.'))
        .unwrap_or(false)
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

fn unique_archive_path(target: &Path) -> PathBuf {
    unique_path(target)
}

fn unique_path(target: &Path) -> PathBuf {
    if !target.exists() {
        return target.to_path_buf();
    }
    let stem = target
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("artifact");
    let extension = target.extension().and_then(|extension| extension.to_str());
    for index in 1.. {
        let candidate_name = match extension {
            Some(extension) => format!("{stem}-{index}.{extension}"),
            None => format!("{stem}-{index}"),
        };
        let candidate = target.with_file_name(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("archive path suffix search is unbounded")
}

fn move_file_to_target(
    state: &AppState,
    source: &str,
    target_dir: &Path,
    archived: bool,
    strategy: ConflictStrategy,
) -> Result<FileMetadata, String> {
    if matches!(strategy, ConflictStrategy::Cancel) {
        return Err("move cancelled".to_owned());
    }
    let source = absolute_doc_path(source)?;
    ensure_regular_file(source)?;
    ensure_under_root(source, &state.paths.canvas_root)?;
    fs::create_dir_all(target_dir).map_err(|error| error.to_string())?;
    let file_name = source
        .file_name()
        .ok_or_else(|| "move source has no filename".to_owned())?;
    let target = target_dir.join(file_name);
    let target = if target.exists() {
        match strategy {
            ConflictStrategy::Replace => {
                fs::remove_file(&target).map_err(|error| error.to_string())?;
                target
            }
            ConflictStrategy::KeepBoth => unique_path(&target),
            ConflictStrategy::Cancel => return Err("move cancelled".to_owned()),
        }
    } else {
        target
    };

    fs::rename(source, &target).map_err(|error| error.to_string())?;
    let mut file = metadata_for_file(&target, &state.paths.canvas_root)?;
    file.archived = archived;
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    conn.execute(
        "UPDATE files SET path = ?1, archived = ?2 WHERE path = ?3",
        params![
            file.path,
            if archived { 1 } else { 0 },
            source.to_string_lossy()
        ],
    )
    .map_err(|error| error.to_string())?;
    upsert_file_state(&conn, &file)?;
    Ok(file)
}

fn ensure_under_root(path: &Path, root: &Path) -> Result<(), String> {
    path.strip_prefix(root)
        .map(|_| ())
        .map_err(|_| "path must live under AgentCanvas root".to_owned())
}

fn format_send_payload(payload: &SendPayload, canvas_root: &Path) -> Result<String, String> {
    let note = payload.note.as_deref().unwrap_or("").trim();
    let note_block = if note.is_empty() {
        String::new()
    } else {
        format!("My note: {note}\n\n")
    };
    let relative_path = relative_canvas_path(&payload.path, canvas_root)?;
    let language = language_from_path(&payload.path);
    let fence = if language.is_empty() {
        "```".to_owned()
    } else {
        format!("```{language}")
    };
    let action = payload.action_verb.trim();
    let action = if action.is_empty() { "Review" } else { action };

    Ok(format!(
        "I'm sending you `{relative_path}` from my AgentCanvas.\n\n{note_block}Contents:\n\n{fence}\n{}\n```\n\nAction: {action}",
        payload.contents
    ))
}

fn relative_canvas_path(path: &str, canvas_root: &Path) -> Result<String, String> {
    let path = Path::new(path);
    let relative = path
        .strip_prefix(canvas_root)
        .map_err(|_| "send payload path must live under AgentCanvas root".to_owned())?;
    Ok(relative.to_string_lossy().into_owned())
}

fn language_from_path(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("md" | "markdown") => "markdown",
        Some("html" | "htm") => "html",
        _ => "",
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
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
            get_project_default_agent,
            set_project_default_agent,
            list_personas,
            get_default_action_verb,
            set_default_action_verb,
            toggle_pin,
            archive_file,
            copy_paths_to_inbox,
            move_file_to_project,
            move_file_to_archive,
            target_file_exists,
            copy_text_to_clipboard,
            reveal_in_finder,
            delete_file,
            send_to_clipboard,
            list_agent_sessions,
            add_agent_session,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_payload_uses_relative_path_fence_note_and_action() {
        let root =
            Path::new("/Users/jessepike/Library/Mobile Documents/com~apple~CloudDocs/AgentCanvas");
        let payload = SendPayload {
            path: root.join("Inbox/test.md").to_string_lossy().into_owned(),
            contents: "# Test\n\nBody".to_owned(),
            note: Some("Tighten this.".to_owned()),
            action_verb: "Revise".to_owned(),
        };

        let formatted = format_send_payload(&payload, root).expect("payload formats");

        assert_eq!(
            formatted,
            "I'm sending you `Inbox/test.md` from my AgentCanvas.\n\nMy note: Tighten this.\n\nContents:\n\n```markdown\n# Test\n\nBody\n```\n\nAction: Revise"
        );
        assert!(!formatted.contains("Path:"));
        assert!(!formatted.contains("/Users/jessepike/Library/Mobile Documents"));
    }

    #[test]
    fn send_payload_omits_empty_note_and_defaults_action() {
        let root =
            Path::new("/Users/jessepike/Library/Mobile Documents/com~apple~CloudDocs/AgentCanvas");
        let payload = SendPayload {
            path: root
                .join("Archive/report.html")
                .to_string_lossy()
                .into_owned(),
            contents: "<h1>Report</h1>".to_owned(),
            note: Some("   ".to_owned()),
            action_verb: " ".to_owned(),
        };

        let formatted = format_send_payload(&payload, root).expect("payload formats");

        assert_eq!(
            formatted,
            "I'm sending you `Archive/report.html` from my AgentCanvas.\n\nContents:\n\n```html\n<h1>Report</h1>\n```\n\nAction: Review"
        );
        assert!(!formatted.contains("My note:"));
    }
}
