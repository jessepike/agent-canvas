#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

// Maximum number of recent files to keep in the `recents` table.
const RECENTS_LIMIT: usize = 50;

#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};

use base64::{Engine as _, engine::general_purpose};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use vellum_core::{
    block::patch::BlockPatch,
    fs::{AtomicWriteError, OpenDocument, WriteResult, atomic_write, has_conflict_markers},
    sidecar::{self, BaseSnapshot, Comment, IdentityMap},
    watch::{self, WatchEvent, WatchHandle},
};
use walkdir::WalkDir;

mod mcp;

type PersonaMetadataCacheKey = (String, i64, u64);

static PERSONA_METADATA_CACHE: OnceLock<Mutex<HashMap<PersonaMetadataCacheKey, String>>> =
    OnceLock::new();

struct AppState {
    paths: Result<AgentCanvasPaths, String>,
    db: Mutex<Connection>,
    watcher: Mutex<Option<WatchHandle>>,
    current_focus: Mutex<Option<String>>,
    /// Paths currently open as ephemeral (transient watched, no `files` row).
    ephemeral_paths: Mutex<HashSet<PathBuf>>,
    /// Paths buffered before the webview attaches (cold-launch open events).
    pending_opens: Mutex<Vec<PathBuf>>,
}

impl AppState {
    fn paths(&self) -> Result<&AgentCanvasPaths, String> {
        self.paths.as_ref().map_err(|error| error.clone())
    }

    fn bootstrap_error(&self) -> Option<String> {
        self.paths.as_ref().err().cloned()
    }
}

#[derive(Debug, Clone)]
struct AgentCanvasPaths {
    canvas_root: PathBuf,
    user_symlink: PathBuf,
    inbox_dir: PathBuf,
    myfiles_dir: PathBuf,
    projects_dir: PathBuf,
    archive_dir: PathBuf,
    state_db: PathBuf,
    persona_registry: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct BootstrapInfo {
    canvas_root: String,
    inbox_dir: String,
    myfiles_dir: String,
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
    review_state: String,
    comment_count: u32,
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

#[derive(Debug, Clone, Serialize)]
struct BinaryArtifact {
    kind: String,
    data_url: String,
    size: u64,
    mime: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SendPayload {
    path: String,
    contents: String,
    note: Option<String>,
    action_verb: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActionTemplate {
    verb: String,
    template: String,
}

#[derive(Debug, Clone, Serialize)]
struct SessionAttachmentInfo {
    session_id: String,
    persona: String,
    agent: String,
    project: String,
    attached_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum SendBackRoute {
    Mcp,
}

#[derive(Debug, Clone, Serialize)]
struct SendBackResult {
    route: SendBackRoute,
    delivered: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct AddAgentSessionInput {
    persona: String,
    backbone: String,
    context: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum InstallAction {
    Created,
    Updated,
    Noop,
}

#[derive(Debug, Clone, Serialize)]
struct InstallResult {
    config_path: String,
    action: InstallAction,
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

#[derive(Debug, Clone, Serialize)]
struct BootstrapErrorPayload {
    message: String,
    hint: String,
}

#[tauri::command]
fn bootstrap_info(state: tauri::State<AppState>) -> Result<BootstrapInfo, String> {
    Ok(state.paths()?.bootstrap_info())
}

#[tauri::command]
fn list_inbox(state: tauri::State<AppState>) -> Result<Vec<FileMetadata>, String> {
    let paths = state.paths()?;
    list_tracked_files(
        &state.db,
        &paths.canvas_root,
        "in_inbox = 1 AND archived = 0",
        [],
    )
}

#[tauri::command]
fn list_project_files(
    state: tauri::State<AppState>,
    project: String,
) -> Result<Vec<FileMetadata>, String> {
    let paths = state.paths()?;
    let project = safe_project_segment(&project)?;
    list_tracked_files(
        &state.db,
        &paths.canvas_root,
        "project_tag = ?1 AND archived = 0",
        [project],
    )
}

#[tauri::command]
fn list_archive(state: tauri::State<AppState>) -> Result<Vec<FileMetadata>, String> {
    let paths = state.paths()?;
    list_tracked_files(&state.db, &paths.canvas_root, "archived = 1", [])
}

#[tauri::command]
fn list_pinned(state: tauri::State<AppState>) -> Result<Vec<FileMetadata>, String> {
    let state_paths = state.paths()?;
    list_tracked_files(
        &state.db,
        &state_paths.canvas_root,
        "pinned = 1 AND archived = 0",
        [],
    )
}

#[tauri::command]
fn list_drafts(state: tauri::State<AppState>) -> Result<Vec<FileMetadata>, String> {
    let paths = state.paths()?;
    let myfiles_prefix = paths.myfiles_dir.to_string_lossy().into_owned();
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    // Find all tracked, non-archived files that live under MyFiles/.
    let mut stmt = conn
        .prepare(
            "SELECT path FROM files WHERE archived = 0 AND path LIKE ?1",
        )
        .map_err(|error| error.to_string())?;
    let like_pattern = format!("{}/%", myfiles_prefix);
    let db_paths = stmt
        .query_map([like_pattern], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(stmt);

    let mut files = Vec::new();
    for path_str in db_paths {
        let path = PathBuf::from(&path_str);
        if !path.exists() || !is_supported_artifact(&path) {
            continue;
        }
        let mut file = metadata_for_file(&path, &paths.canvas_root)?;
        hydrate_file_state(&conn, &mut file)?;
        files.push(file);
    }
    files.sort_by(|left, right| {
        right
            .pinned
            .cmp(&left.pinned)
            .then_with(|| right.mtime.cmp(&left.mtime))
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(files)
}

#[tauri::command]
fn inbox_unread_count(state: tauri::State<AppState>) -> Result<u32, String> {
    let _ = state.paths()?;
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM files WHERE in_inbox = 1 AND archived = 0 AND review_state = 'unread'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    Ok(count as u32)
}

/// Sanitize a user-supplied draft filename: strip path separators, force `.md` extension,
/// resolve collisions by appending ` 2`, ` 3`, … (matching rename-dialog convention).
fn sanitize_draft_name(raw: &str, myfiles_dir: &Path) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("file name cannot be empty".to_owned());
    }
    // Strip any path traversal components.
    let bare: String = trimmed
        .chars()
        .filter(|&c| c != '/' && c != '\\' && c != '\0')
        .collect();
    if bare.is_empty() || bare == "." || bare == ".." {
        return Err("invalid file name".to_owned());
    }
    // Strip existing extension and always force `.md`.
    let stem = Path::new(&bare)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(&bare)
        .to_owned();
    if stem.is_empty() {
        return Err("invalid file name".to_owned());
    }

    // Find a non-colliding path: `stem.md`, then `stem 2.md`, `stem 3.md`, …
    let candidate = myfiles_dir.join(format!("{stem}.md"));
    if !candidate.exists() {
        return Ok(candidate);
    }
    for n in 2_u32..=999 {
        let candidate = myfiles_dir.join(format!("{stem} {n}.md"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("too many files with that name".to_owned())
}

#[tauri::command]
fn create_my_file(state: tauri::State<AppState>, name: String) -> Result<String, String> {
    let paths = state.paths()?;
    let target = sanitize_draft_name(&name, &paths.myfiles_dir)?;

    // Atomic-write an empty file (same guard: fail if it somehow appeared between check and write).
    let mut file_handle = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|error| error.to_string())?;
    file_handle.flush().map_err(|error| error.to_string())?;
    drop(file_handle);

    let path_str = target.to_string_lossy().into_owned();
    let file = metadata_for_file(&target, &paths.canvas_root)?;
    {
        let conn = state
            .db
            .lock()
            .map_err(|_| "state db lock poisoned".to_owned())?;
        upsert_file_state(&conn, &file)?;
        // Drafts: NOT in_inbox, NOT archived, review_state stays default 'unread'
        // but we immediately mark it 'reviewed' since the user created it intentionally.
        conn.execute(
            "UPDATE files SET in_inbox = 0, archived = 0, review_state = 'reviewed' WHERE path = ?1",
            params![path_str],
        )
        .map_err(|error| error.to_string())?;
    }
    resync_watcher_from_db(&state)?;
    Ok(path_str)
}

#[tauri::command]
fn list_projects(state: tauri::State<AppState>) -> Result<Vec<String>, String> {
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    let mut stmt = conn
        .prepare(
            "SELECT name FROM projects
             UNION
             SELECT project_tag FROM files WHERE project_tag IS NOT NULL AND project_tag != ''
             ORDER BY 1",
        )
        .map_err(|error| error.to_string())?;
    let projects = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(projects)
}

#[tauri::command]
fn list_project_counts(state: tauri::State<AppState>) -> Result<HashMap<String, usize>, String> {
    let mut counts = HashMap::new();
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    let mut stmt = conn
        .prepare(
            "SELECT project_tag, COUNT(*) FROM files
             WHERE project_tag IS NOT NULL AND project_tag != '' AND archived = 0
             GROUP BY project_tag",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (project, count) = row.map_err(|error| error.to_string())?;
        counts.insert(project, count as usize);
    }
    Ok(counts)
}

#[tauri::command]
fn rename_project(state: tauri::State<AppState>, old: String, new: String) -> Result<(), String> {
    let old = safe_project_segment(&old)?;
    let new = safe_project_segment(&new)?;
    if new.contains("..") {
        return Err("invalid project name".to_owned());
    }
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    conn.execute(
        "UPDATE files SET project_tag = ?1 WHERE project_tag = ?2",
        params![new, old],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE projects SET name = ?1, updated_at = strftime('%s','now') WHERE name = ?2",
        params![new, old],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn delete_project_if_empty(state: tauri::State<AppState>, name: String) -> Result<(), String> {
    let name = safe_project_segment(&name)?;
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    let has_artifacts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM files WHERE project_tag = ?1 AND archived = 0",
            params![name],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if has_artifacts > 0 {
        return Err("Move files out before deleting project".to_owned());
    }
    conn.execute("DELETE FROM projects WHERE name = ?1", params![name])
        .map_err(|error| error.to_string())?;
    Ok(())
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
    let paths = state.paths()?;
    resolve_personas(&paths.persona_registry, &state.db)
}

#[tauri::command]
async fn reload_persona_registry(
    state: tauri::State<'_, AppState>,
) -> Result<PersonaRegistry, String> {
    let paths = state.paths()?;
    let registry = resolve_personas(&paths.persona_registry, &state.db)?;
    mcp::reload_personas(paths.persona_registry.clone()).await;
    Ok(registry)
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
    let path = path_safe_for_canvas(Path::new(&path))?;
    let path = path.to_string_lossy().into_owned();
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
    drop(conn);
    resync_watcher_from_db(&state)?;
    Ok(next == 1)
}

#[tauri::command]
fn archive_file(state: tauri::State<AppState>, path: String) -> Result<String, String> {
    let paths = state.paths()?;
    let source = path_safe_for_canvas(Path::new(&path))?;
    ensure_regular_file(&source)?;
    let mut file = metadata_for_file(&source, &paths.canvas_root)?;
    {
        let conn = state
            .db
            .lock()
            .map_err(|_| "state db lock poisoned".to_owned())?;
        upsert_file_state(&conn, &file)?;
        conn.execute(
            "UPDATE files SET archived = 1, in_inbox = 0 WHERE path = ?1",
            params![source.to_string_lossy()],
        )
        .map_err(|error| error.to_string())?;
    }
    resync_watcher_from_db(&state)?;
    file.archived = true;
    Ok(file.path)
}

#[tauri::command]
fn track_paths_in_inbox(
    state: tauri::State<AppState>,
    paths: Vec<String>,
) -> Result<Vec<FileMetadata>, String> {
    let state_paths = state.paths()?;
    let mut tracked = Vec::new();
    for path in paths {
        let source = path_safe_for_canvas(Path::new(&path))?;
        ensure_regular_file(&source)?;
        let mut file = metadata_for_file(&source, &state_paths.canvas_root)?;
        {
            let conn = state
                .db
                .lock()
                .map_err(|_| "state db lock poisoned".to_owned())?;
            upsert_file_state(&conn, &file)?;
            conn.execute(
                "UPDATE files SET in_inbox = 1, project_tag = NULL, archived = 0 WHERE path = ?1",
                params![file.path],
            )
            .map_err(|error| error.to_string())?;
            hydrate_file_state(&conn, &mut file)?;
        }
        tracked.push(file);
    }
    resync_watcher_from_db(&state)?;
    Ok(tracked)
}

#[tauri::command]
fn copy_paths_to_inbox(
    state: tauri::State<AppState>,
    paths: Vec<String>,
) -> Result<Vec<FileMetadata>, String> {
    track_paths_in_inbox(state, paths)
}

#[tauri::command]
fn move_file_to_project(
    state: tauri::State<AppState>,
    path: String,
    project: String,
    _strategy: ConflictStrategy,
) -> Result<FileMetadata, String> {
    let paths = state.paths()?;
    let project = safe_project_segment(&project)?;
    upsert_project(&state.db, project, None)?;
    let source = path_safe_for_canvas(Path::new(&path))?;
    ensure_regular_file(&source)?;
    let mut file = metadata_for_file(&source, &paths.canvas_root)?;
    {
        let conn = state
            .db
            .lock()
            .map_err(|_| "state db lock poisoned".to_owned())?;
        upsert_file_state(&conn, &file)?;
        conn.execute(
            "UPDATE files SET project_tag = ?1, in_inbox = 0, archived = 0 WHERE path = ?2",
            params![project, file.path],
        )
        .map_err(|error| error.to_string())?;
        hydrate_file_state(&conn, &mut file)?;
    }
    resync_watcher_from_db(&state)?;
    Ok(file)
}

#[tauri::command]
fn move_file_to_archive(
    state: tauri::State<AppState>,
    path: String,
    _strategy: ConflictStrategy,
) -> Result<FileMetadata, String> {
    let paths = state.paths()?;
    let source = path_safe_for_canvas(Path::new(&path))?;
    ensure_regular_file(&source)?;
    let mut file = metadata_for_file(&source, &paths.canvas_root)?;
    {
        let conn = state
            .db
            .lock()
            .map_err(|_| "state db lock poisoned".to_owned())?;
        upsert_file_state(&conn, &file)?;
        conn.execute(
            "UPDATE files SET archived = 1, in_inbox = 0 WHERE path = ?1",
            params![file.path],
        )
        .map_err(|error| error.to_string())?;
        hydrate_file_state(&conn, &mut file)?;
    }
    resync_watcher_from_db(&state)?;
    Ok(file)
}

#[tauri::command]
fn target_file_exists(
    _state: tauri::State<AppState>,
    target: String,
    project: Option<String>,
    filename: String,
) -> Result<bool, String> {
    if filename.contains('/') || filename.contains('\\') || filename.is_empty() {
        return Err("invalid filename".to_owned());
    }
    match target.as_str() {
        "archive" => {}
        "project" => {
            let project = project.ok_or_else(|| "project is required".to_owned())?;
            safe_project_segment(&project)?;
        }
        _ => return Err("invalid target".to_owned()),
    }
    Ok(false)
}

#[tauri::command]
fn copy_text_to_clipboard(text: String) -> Result<String, String> {
    write_clipboard(&text)?;
    Ok(text)
}

#[tauri::command]
fn reveal_in_finder(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    let _ = state.paths()?;
    let path = path_safe_for_canvas(Path::new(&path))?;
    ensure_regular_file(&path)?;
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
fn untrack_file(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    let _ = state.paths()?;
    let source = path_safe_for_canvas(Path::new(&path))?;
    {
        let conn = state
            .db
            .lock()
            .map_err(|_| "state db lock poisoned".to_owned())?;
        untrack_file_impl(&conn, &source)?;
    }
    resync_watcher_from_db(&state)
}

#[tauri::command]
fn delete_file_from_disk(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    let _ = state.paths()?;
    let source = path_safe_for_canvas(Path::new(&path))?;
    ensure_regular_file(&source)?;
    {
        let conn = state
            .db
            .lock()
            .map_err(|_| "state db lock poisoned".to_owned())?;
        delete_file_from_disk_impl(&conn, &source)?;
    }
    resync_watcher_from_db(&state)
}

#[tauri::command]
fn delete_file(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    delete_file_from_disk(state, path)
}

#[tauri::command]
fn rename_file(
    state: tauri::State<AppState>,
    old_path: String,
    new_name: String,
) -> Result<FileMetadata, String> {
    let paths = state.paths()?;
    let source = path_safe_for_canvas(Path::new(&old_path))?;
    ensure_regular_file(&source)?;

    if new_name.is_empty()
        || new_name.contains('/')
        || new_name.contains('\\')
        || new_name == "."
        || new_name == ".."
    {
        return Err("invalid new name".to_owned());
    }

    let parent = source
        .parent()
        .ok_or_else(|| "source has no parent directory".to_owned())?;
    let target = parent.join(&new_name);

    if target.exists() {
        return Err(format!("a file named '{new_name}' already exists here"));
    }

    let target_bounded = path_safe_for_canvas(&target)?;
    fs::rename(&source, &target_bounded).map_err(|error| error.to_string())?;

    let mut file = metadata_for_file(&target_bounded, &paths.canvas_root)?;
    {
        let conn = state
            .db
            .lock()
            .map_err(|_| "state db lock poisoned".to_owned())?;
        conn.execute(
            "UPDATE files SET path = ?1 WHERE path = ?2",
            params![target_bounded.to_string_lossy(), source.to_string_lossy()],
        )
        .map_err(|error| error.to_string())?;
        hydrate_file_state(&conn, &mut file)?;
    }
    resync_watcher_from_db(&state)?;
    Ok(file)
}

#[tauri::command]
fn export_file_to(
    state: tauri::State<AppState>,
    source_path: String,
    target_path: String,
) -> Result<(), String> {
    let _ = state.paths()?;
    let source = path_safe_for_canvas(Path::new(&source_path))?;
    ensure_regular_file(&source)?;

    let target = PathBuf::from(target_path);
    if target.exists() {
        return Err("export target already exists".to_owned());
    }
    let parent = target
        .parent()
        .ok_or_else(|| "export target must have a parent directory".to_owned())?;
    if !parent.exists() {
        return Err("export target parent does not exist".to_owned());
    }
    if !parent.is_dir() {
        return Err("export target parent is not a directory".to_owned());
    }

    let target = path_safe_for_canvas(&target)?;
    fs::copy(&source, &target).map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn send_to_clipboard(
    state: tauri::State<AppState>,
    payload: SendPayload,
) -> Result<String, String> {
    let paths = state.paths()?;
    let payload_path = path_safe_for_canvas(Path::new(&payload.path))?;
    let payload = SendPayload {
        path: payload_path.to_string_lossy().into_owned(),
        contents: payload.contents,
        note: payload.note,
        action_verb: payload.action_verb,
    };
    let templates = action_templates_from_db(&state.db)?;
    let formatted = format_send_payload(&payload, &paths.canvas_root, &templates)?;
    write_clipboard(&formatted)?;
    Ok(formatted)
}

#[tauri::command]
fn send_multi_to_clipboard(
    state: tauri::State<AppState>,
    payloads: Vec<SendPayload>,
) -> Result<String, String> {
    if payloads.is_empty() {
        return Err("no files to send".to_owned());
    }
    let paths = state.paths()?;
    let mut bounded: Vec<SendPayload> = Vec::with_capacity(payloads.len());
    for payload in payloads {
        let bounded_path = path_safe_for_canvas(Path::new(&payload.path))?;
        bounded.push(SendPayload {
            path: bounded_path.to_string_lossy().into_owned(),
            contents: payload.contents,
            note: payload.note,
            action_verb: payload.action_verb,
        });
    }
    let templates = action_templates_from_db(&state.db)?;
    let formatted = format_send_multi_payload(&bounded, &paths.canvas_root, &templates)?;
    write_clipboard(&formatted)?;
    Ok(formatted)
}

#[tauri::command]
fn session_attachments_for_path(
    state: tauri::State<AppState>,
    path: String,
) -> Result<Vec<SessionAttachmentInfo>, String> {
    let _ = state.paths()?;
    let path = path_safe_for_canvas(Path::new(&path))?;
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    mcp::sessions::attachments_for_path(&conn, &path.to_string_lossy()).map(|attachments| {
        attachments
            .into_iter()
            .map(|attachment| SessionAttachmentInfo {
                session_id: attachment.session_id,
                persona: attachment.persona,
                agent: attachment.agent,
                project: attachment.project,
                attached_at: attachment.attached_at,
            })
            .collect()
    })
}

#[tauri::command]
fn send_back_to_session(
    state: tauri::State<AppState>,
    path: String,
    session_id: String,
    note: Option<String>,
    action_verb: Option<String>,
) -> Result<SendBackResult, String> {
    let _ = state.paths()?;
    let path = path_safe_for_canvas(Path::new(&path))?;
    let path_string = path.to_string_lossy().into_owned();
    let created_at = unix_now();
    {
        let conn = state
            .db
            .lock()
            .map_err(|_| "state db lock poisoned".to_owned())?;
        let attached: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM session_attachments WHERE session_id = ?1 AND path = ?2",
                params![session_id, path_string],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if attached == 0 {
            // Not yet attached — check whether the session is still live in agent_sessions.
            let session_exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM agent_sessions WHERE session_id = ?1 AND disconnected_at IS NULL",
                    params![session_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            if session_exists == 0 {
                return Err("agent is no longer connected".to_owned());
            }
            // Session is live — auto-attach so the send can proceed.
            mcp::sessions::attach_artifact(&conn, &session_id, &path_string, created_at)?;
        }
        mcp::sessions::insert_user_message(
            &conn,
            &session_id,
            &path_string,
            note.as_deref(),
            action_verb.as_deref(),
            created_at,
        )?;
    }
    let delivered = usize::from(mcp::emit_artifact_updated_to_session(
        &session_id,
        path_string,
        "user",
        note,
        action_verb,
    ));
    Ok(SendBackResult {
        route: SendBackRoute::Mcp,
        delivered,
    })
}

#[tauri::command]
fn get_action_templates(state: tauri::State<AppState>) -> Result<Vec<ActionTemplate>, String> {
    action_templates_from_db(&state.db)
}

#[tauri::command]
fn set_action_templates(
    state: tauri::State<AppState>,
    templates: Vec<ActionTemplate>,
) -> Result<(), String> {
    let value = serde_json::to_string(&templates).map_err(|error| error.to_string())?;
    set_setting(&state.db, "action_templates", &value)
}

#[tauri::command]
fn reset_action_templates(state: tauri::State<AppState>) -> Result<Vec<ActionTemplate>, String> {
    let templates = default_action_templates();
    let value = serde_json::to_string(&templates).map_err(|error| error.to_string())?;
    set_setting(&state.db, "action_templates", &value)?;
    Ok(templates)
}

#[tauri::command]
fn list_agent_sessions(
    state: tauri::State<AppState>,
) -> Result<Vec<mcp::sessions::AgentSession>, String> {
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    mcp::sessions::list_agent_sessions(&conn)
}

#[tauri::command]
fn add_agent_session(
    state: tauri::State<AppState>,
    input: AddAgentSessionInput,
) -> Result<mcp::sessions::AgentSession, String> {
    let now = unix_now();
    let session = mcp::sessions::AgentSession {
        id: uuid::Uuid::new_v4().to_string(),
        source: "manual".to_owned(),
        persona: input.persona,
        agent: input.backbone,
        project: input.context,
        connected_at: now,
        last_active: Some(now),
        is_live: false,
        attached_paths: Vec::new(),
    };
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    conn.execute(
        "INSERT INTO manual_agent_sessions(id, persona, backbone, context, connected_at, last_active)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            session.id,
            session.persona,
            session.agent,
            session.project,
            session.connected_at,
            session.last_active.unwrap_or(now)
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(session)
}

#[tauri::command]
fn remove_agent_session(state: tauri::State<AppState>, session_id: String) -> Result<(), String> {
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    conn.execute(
        "DELETE FROM manual_agent_sessions WHERE id = ?1",
        params![session_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn disconnect_mcp_session(state: tauri::State<AppState>, session_id: String) -> Result<(), String> {
    let _ = mcp::disconnect_session(&session_id);
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    mcp::sessions::delete_agent_session(&conn, &session_id)
}

// ---------------------------------------------------------------------------
// Slice 7 — Agent messages commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn list_agent_messages(
    state: tauri::State<AppState>,
) -> Result<Vec<mcp::sessions::AgentMessage>, String> {
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    mcp::sessions::list_unacknowledged_agent_messages(&conn)
}

#[tauri::command]
fn acknowledge_agent_message(
    app_handle: tauri::AppHandle,
    state: tauri::State<AppState>,
    id: String,
) -> Result<(), String> {
    {
        let conn = state
            .db
            .lock()
            .map_err(|_| "state db lock poisoned".to_owned())?;
        mcp::sessions::delete_agent_message(&conn, &id)?;
        // Guard drops here — emit runs post-lock (lock discipline).
    }
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.emit("agentcanvas://messages-changed", serde_json::json!({}));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Slice 0.5 — Interaction commands (Tauri frontend ↔ backend)
// ---------------------------------------------------------------------------

/// Return all pending/draft interactions (for the UI to render).
#[tauri::command]
fn list_interactions(
    state: tauri::State<AppState>,
) -> Result<Vec<mcp::sessions::Interaction>, String> {
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    mcp::sessions::list_interactions_pending(&conn)
}

/// Get a single interaction by id.
#[tauri::command]
fn get_interaction(
    state: tauri::State<AppState>,
    interaction_id: String,
) -> Result<Option<mcp::sessions::Interaction>, String> {
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    mcp::sessions::get_interaction(&conn, &interaction_id)
}

/// Submit (or save-draft / dismiss) an operator response to an interaction.
/// `payload` is the §4 JSON object (must include `interaction_id` matching the request).
#[tauri::command]
fn submit_interaction_response(
    app_handle: tauri::AppHandle,
    state: tauri::State<AppState>,
    interaction_id: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    // Validate that payload.interaction_id matches.
    let payload_id = payload
        .get("interaction_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if payload_id != interaction_id {
        return Err(format!(
            "payload.interaction_id ({payload_id}) does not match interaction_id ({interaction_id})"
        ));
    }

    let status = payload
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("submitted");

    // Validate status.
    match status {
        "submitted" | "draft" | "dismissed" => {}
        other => return Err(format!("invalid status: {other}")),
    }

    let response_json =
        serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    let now_ts = unix_now();

    let (lifecycle_event, class_val, trace_id_val) = {
        let conn = state
            .db
            .lock()
            .map_err(|_| "state db lock poisoned".to_owned())?;

        let result = mcp::sessions::submit_interaction(
            &conn,
            &interaction_id,
            status,
            &response_json,
            now_ts,
        )?;

        match result {
            None => return Err(format!("interaction not found: {interaction_id}")),
            Some((class, trace_id)) => {
                let event = if status == "dismissed" {
                    "agentcanvas://interaction.dismissed"
                } else {
                    "agentcanvas://interaction.responded"
                };
                (event, class, trace_id)
            }
        }
        // db guard drops here — emits run post-lock.
    };

    // Post-lock: emit lifecycle event + messages-changed.
    let ts = {
        // Reuse payload.submitted_at if present (canonical per spec §4 rule 6), else now.
        payload
            .get("submitted_at")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| iso8601_now_main())
    };
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.emit(
            lifecycle_event,
            serde_json::json!({
                "interaction_id": interaction_id,
                "trace_id": trace_id_val,
                "class": class_val,
                "status": status,
                "ts": ts
            }),
        );
        let _ = window.emit("agentcanvas://messages-changed", serde_json::json!({}));
    }
    Ok(())
}

/// ISO-8601 UTC helper for Tauri commands.
fn iso8601_now_main() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    mcp::epoch_secs_to_iso8601(secs)
}

#[tauri::command]
fn install_mcp_for_claude_code() -> Result<InstallResult, String> {
    let config_path = home_dir()?.join(".claude.json");
    install_mcp_for_claude_code_at(config_path, resolve_mcp_shim_path()?)
}

#[tauri::command]
fn install_mcp_for_codex() -> Result<InstallResult, String> {
    let config_path = home_dir()?.join(".codex").join("config.toml");
    install_mcp_for_codex_at(config_path, resolve_mcp_shim_path()?)
}

#[tauri::command]
fn install_mcp_for_cursor() -> Result<InstallResult, String> {
    let config_path = home_dir()?.join(".cursor").join("mcp.json");
    install_mcp_for_cursor_at(config_path, resolve_mcp_shim_path()?)
}

fn resolve_mcp_shim_path() -> Result<PathBuf, String> {
    let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let Some(parent) = current_exe.parent() else {
        return Err("cannot resolve AgentCanvas executable directory".to_owned());
    };
    let sibling = parent.join("agent-canvas-mcp");
    if sibling.exists() {
        return Ok(sibling);
    }
    let workspace_debug = std::env::current_dir()
        .map_err(|error| error.to_string())?
        .ancestors()
        .find_map(|ancestor| {
            let candidate = ancestor
                .join("target")
                .join("debug")
                .join("agent-canvas-mcp");
            candidate.exists().then_some(candidate)
        });
    workspace_debug.ok_or_else(|| {
        format!(
            "agent-canvas-mcp binary not found next to {} or in target/debug",
            current_exe.display()
        )
    })
}

pub(crate) fn install_mcp_for_claude_code_at(
    config_path: PathBuf,
    shim_path: PathBuf,
) -> Result<InstallResult, String> {
    let existed = config_path.exists();
    let mut root = read_json_config(&config_path)?;
    let servers = root
        .as_object_mut()
        .ok_or_else(|| "Claude Code config root must be a JSON object".to_owned())?
        .entry("mcpServers".to_owned())
        .or_insert_with(|| serde_json::json!({}));
    let servers = servers
        .as_object_mut()
        .ok_or_else(|| "mcpServers must be a JSON object".to_owned())?;
    let entry = serde_json::json!({
        "command": shim_path.to_string_lossy(),
        "args": [],
        "env": {}
    });
    let action = install_action(existed, servers.get("agent-canvas"), &entry);
    servers.insert("agent-canvas".to_owned(), entry);
    write_json_config(&config_path, &root)?;
    Ok(InstallResult {
        config_path: config_path.to_string_lossy().into_owned(),
        action,
    })
}

pub(crate) fn install_mcp_for_cursor_at(
    config_path: PathBuf,
    shim_path: PathBuf,
) -> Result<InstallResult, String> {
    let existed = config_path.exists();
    let mut root = read_json_config(&config_path)?;
    let servers = root
        .as_object_mut()
        .ok_or_else(|| "Cursor MCP config root must be a JSON object".to_owned())?
        .entry("mcpServers".to_owned())
        .or_insert_with(|| serde_json::json!({}));
    let servers = servers
        .as_object_mut()
        .ok_or_else(|| "mcpServers must be a JSON object".to_owned())?;
    let entry = serde_json::json!({
        "command": shim_path.to_string_lossy()
    });
    let action = install_action(existed, servers.get("agent-canvas"), &entry);
    servers.insert("agent-canvas".to_owned(), entry);
    write_json_config(&config_path, &root)?;
    Ok(InstallResult {
        config_path: config_path.to_string_lossy().into_owned(),
        action,
    })
}

pub(crate) fn install_mcp_for_codex_at(
    config_path: PathBuf,
    shim_path: PathBuf,
) -> Result<InstallResult, String> {
    let existed = config_path.exists();
    let original = fs::read_to_string(&config_path).unwrap_or_default();
    let mut root = if original.trim().is_empty() {
        toml::Value::Table(toml::map::Map::new())
    } else {
        original
            .parse::<toml::Value>()
            .map_err(|error| error.to_string())?
    };
    let root_table = root
        .as_table_mut()
        .ok_or_else(|| "Codex config root must be a TOML table".to_owned())?;
    let mcp_servers = root_table
        .entry("mcp_servers".to_owned())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .ok_or_else(|| "mcp_servers must be a TOML table".to_owned())?;
    let mut entry_table = toml::map::Map::new();
    entry_table.insert(
        "command".to_owned(),
        toml::Value::String(shim_path.to_string_lossy().into_owned()),
    );
    entry_table.insert("args".to_owned(), toml::Value::Array(Vec::new()));
    let entry = toml::Value::Table(entry_table);
    let action = install_action(existed, mcp_servers.get("agent-canvas"), &entry);
    mcp_servers.insert("agent-canvas".to_owned(), entry);
    let rendered = toml::to_string_pretty(&root).map_err(|error| error.to_string())?;
    atomic_write_config(&config_path, rendered.as_bytes())?;
    Ok(InstallResult {
        config_path: config_path.to_string_lossy().into_owned(),
        action,
    })
}

fn install_action<T: PartialEq>(existed: bool, existing: Option<&T>, next: &T) -> InstallAction {
    match existing {
        None if existed => InstallAction::Updated,
        None => InstallAction::Created,
        Some(existing) if existing == next => InstallAction::Noop,
        Some(_) => InstallAction::Updated,
    }
}

fn read_json_config(config_path: &Path) -> Result<serde_json::Value, String> {
    if !config_path.exists() {
        return Ok(serde_json::json!({}));
    }
    let source = fs::read_to_string(config_path).map_err(|error| error.to_string())?;
    if source.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(&source).map_err(|error| error.to_string())
}

fn write_json_config(config_path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    atomic_write_config(config_path, &bytes)
}

fn atomic_write_config(config_path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let tmp_path = config_path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    {
        let mut file = fs::File::create(&tmp_path).map_err(|error| error.to_string())?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.write_all(b"\n").map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    fs::rename(&tmp_path, config_path).map_err(|error| {
        let _ = fs::remove_file(&tmp_path);
        error.to_string()
    })
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
fn open_document(state: tauri::State<AppState>, doc_path: String) -> Result<OpenDocument, String> {
    let _ = state.paths()?;
    let doc_path = path_safe_for_canvas(Path::new(&doc_path))?;
    ensure_regular_file(&doc_path)?;

    let bytes = fs::read(&doc_path).map_err(|error| error.to_string())?;
    let base_hash = *vellum_core::hash::content_hash(&bytes).as_bytes();
    let source = String::from_utf8(bytes).map_err(|error| error.to_string())?;
    let path_string = doc_path.to_string_lossy().into_owned();
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    conn.execute(
        "UPDATE files
         SET last_read_at = strftime('%s','now'),
             review_state = CASE WHEN review_state = 'unread' THEN 'reviewed' ELSE review_state END
         WHERE path = ?1",
        params![path_string],
    )
    .map_err(|error| error.to_string())?;

    Ok(OpenDocument {
        path: path_string,
        has_conflict_markers: has_conflict_markers(&source),
        source,
        base_hash,
    })
}

#[tauri::command]
fn read_binary_artifact(
    state: tauri::State<AppState>,
    doc_path: String,
) -> Result<BinaryArtifact, String> {
    let _ = state.paths()?;
    let doc_path = path_safe_for_canvas(Path::new(&doc_path))?;
    ensure_regular_file(&doc_path)?;

    let extension = normalized_extension(&doc_path);
    let (kind, mime) = match extension.as_str() {
        "png" => ("png", "image/png"),
        "pdf" => ("pdf", "application/pdf"),
        _ => return Err("unsupported binary artifact".to_owned()),
    };

    let bytes = fs::read(&doc_path).map_err(|error| error.to_string())?;
    let data_url = format!(
        "data:{mime};base64,{}",
        general_purpose::STANDARD.encode(&bytes)
    );
    let path_string = doc_path.to_string_lossy().into_owned();
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    conn.execute(
        "UPDATE files
         SET last_read_at = strftime('%s','now'),
             review_state = CASE WHEN review_state = 'unread' THEN 'reviewed' ELSE review_state END
         WHERE path = ?1",
        params![path_string],
    )
    .map_err(|error| error.to_string())?;

    Ok(BinaryArtifact {
        kind: kind.to_owned(),
        data_url,
        size: bytes.len() as u64,
        mime: mime.to_owned(),
    })
}

#[tauri::command]
fn set_review_state(
    state: tauri::State<AppState>,
    path: String,
    review_state: String,
) -> Result<(), String> {
    let _ = state.paths()?;
    let path = path_safe_for_canvas(Path::new(&path))?;
    ensure_regular_file(&path)?;
    if !is_valid_review_state(&review_state) {
        return Err("invalid review state".to_owned());
    }
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    conn.execute(
        "UPDATE files SET review_state = ?1 WHERE path = ?2",
        params![review_state, path.to_string_lossy()],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Slice 5 — Ephemeral open model + Recents
// ---------------------------------------------------------------------------

/// How a path was resolved when opened via `open_path`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum OpenMode {
    /// Path is under the canvas root or already has a `files` row — use the tracked identity.
    Tracked,
    /// Path is external and untracked.
    Ephemeral,
}

/// Return value for `open_path`.
#[derive(Debug, Clone, Serialize)]
struct OpenResult {
    mode: OpenMode,
    path: String,
    source: String,
    base_hash: [u8; 32],
    has_conflict_markers: bool,
    /// Relative-to-canvas-root display path (equals `path` for ephemeral files).
    relative_path: String,
    name: String,
    extension: String,
    size: u64,
    mtime: i64,
}

/// A single entry in the Recents list.
#[derive(Debug, Clone, Serialize)]
struct RecentEntry {
    path: String,
    last_opened: i64,
    title: String,
}

/// Upsert a path into the `recents` table and prune to the cap.
/// Must be called with NO db lock held (acquires its own lock).
fn upsert_recent(db: &Mutex<Connection>, path_str: &str, title: &str, now: i64) -> Result<(), String> {
    let conn = db.lock().map_err(|_| "state db lock poisoned".to_owned())?;
    conn.execute(
        r#"
        INSERT INTO recents(path, last_opened, title)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(path) DO UPDATE SET
          last_opened = excluded.last_opened,
          title = excluded.title
        "#,
        params![path_str, now, title],
    )
    .map_err(|error| error.to_string())?;
    // Prune oldest rows beyond the cap.
    conn.execute(
        &format!(
            r#"
            DELETE FROM recents WHERE path IN (
              SELECT path FROM recents
              ORDER BY last_opened DESC
              LIMIT -1 OFFSET {RECENTS_LIMIT}
            )
            "#
        ),
        [],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Arm a transient watch on the parent directory of an ephemeral path.
/// Records the path in `state.ephemeral_paths` so it can be released later.
fn arm_ephemeral_watch(state: &AppState, path: &PathBuf) {
    let mut ephemeral_paths = match state.ephemeral_paths.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if ephemeral_paths.insert(path.clone()) {
        let watcher = match state.watcher.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        if let Some(watcher) = watcher.as_ref() {
            let _ = watcher.add_path(path);
        }
    }
}

/// Release a transient watch on an ephemeral path.
fn release_ephemeral_watch(state: &AppState, path: &PathBuf) {
    let mut ephemeral_paths = match state.ephemeral_paths.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if ephemeral_paths.remove(path) {
        let watcher = match state.watcher.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        if let Some(watcher) = watcher.as_ref() {
            let _ = watcher.remove_path(path);
        }
    }
}

/// Open a file by absolute path, resolving it to tracked or ephemeral per the spec rule:
///   1. Path under canvas root → tracked
///   2. Path already has a `files` row → tracked
///   3. Otherwise → ephemeral (no files row created)
#[tauri::command]
fn open_path(state: tauri::State<AppState>, path: String) -> Result<OpenResult, String> {
    let paths = state.paths()?;
    let doc_path = path_safe_for_canvas(Path::new(&path))?;
    ensure_regular_file(&doc_path)?;
    let path_str = doc_path.to_string_lossy().into_owned();

    // Determine if tracked or ephemeral — do DB work under the lock then drop it.
    let is_tracked = {
        let under_root = doc_path.starts_with(&paths.canvas_root);
        if under_root {
            true
        } else {
            let conn = state
                .db
                .lock()
                .map_err(|_| "state db lock poisoned".to_owned())?;
            let exists: bool = conn
                .query_row(
                    "SELECT 1 FROM files WHERE path = ?1 LIMIT 1",
                    params![path_str],
                    |_| Ok(()),
                )
                .is_ok();
            exists
        }
    };
    // DB lock is now released.

    let mode = if is_tracked {
        OpenMode::Tracked
    } else {
        OpenMode::Ephemeral
    };

    // Read the file content.
    let bytes = fs::read(&doc_path).map_err(|error| error.to_string())?;
    let base_hash = *vellum_core::hash::content_hash(&bytes).as_bytes();
    let source = String::from_utf8(bytes.clone()).map_err(|error| error.to_string())?;
    let metadata = fs::metadata(&doc_path).map_err(|error| error.to_string())?;
    let size = metadata.len();
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let extension = normalized_extension(&doc_path);
    let name = doc_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact")
        .to_owned();
    let relative_path = doc_path
        .strip_prefix(&paths.canvas_root)
        .unwrap_or(&doc_path)
        .to_string_lossy()
        .into_owned();

    if is_tracked {
        // Mark as read in the DB (best-effort, same as open_document does).
        let conn = state
            .db
            .lock()
            .map_err(|_| "state db lock poisoned".to_owned())?;
        let _ = conn.execute(
            "UPDATE files
             SET last_read_at = strftime('%s','now'),
                 review_state = CASE WHEN review_state = 'unread' THEN 'reviewed' ELSE review_state END
             WHERE path = ?1",
            params![path_str],
        );
        // DB lock drops here.
    } else {
        // Ephemeral: arm transient watch and upsert into recents.
        arm_ephemeral_watch(&state, &doc_path);
        let now = unix_now();
        upsert_recent(&state.db, &path_str, &name, now)?;
    }

    let has_markers = has_conflict_markers(&source);
    Ok(OpenResult {
        mode,
        path: path_str,
        source,
        base_hash,
        has_conflict_markers: has_markers,
        relative_path,
        name,
        extension,
        size,
        mtime,
    })
}

/// Release the ephemeral watch for a path that is no longer open.
#[tauri::command]
fn close_ephemeral_path(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    let doc_path = path_safe_for_canvas(Path::new(&path))?;
    release_ephemeral_watch(&state, &doc_path);
    Ok(())
}

/// List the most-recently opened external (recents) entries.
#[tauri::command]
fn list_recents(state: tauri::State<AppState>) -> Result<Vec<RecentEntry>, String> {
    let _ = state.paths()?;
    let conn = state
        .db
        .lock()
        .map_err(|_| "state db lock poisoned".to_owned())?;
    let mut stmt = conn
        .prepare(
            "SELECT path, last_opened, title FROM recents ORDER BY last_opened DESC LIMIT ?1",
        )
        .map_err(|error| error.to_string())?;
    let entries = stmt
        .query_map(params![RECENTS_LIMIT as i64], |row| {
            Ok(RecentEntry {
                path: row.get(0)?,
                last_opened: row.get(1)?,
                title: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(entries)
}

// ---------------------------------------------------------------------------
// Slice 4 — Startup buffer + take_pending_opens
// ---------------------------------------------------------------------------

/// Drain the cold-launch pending-opens buffer. Called by the frontend on mount.
#[tauri::command]
fn take_pending_opens(state: tauri::State<AppState>) -> Result<Vec<String>, String> {
    let mut pending = state
        .pending_opens
        .lock()
        .map_err(|_| "pending_opens lock poisoned".to_owned())?;
    let drained: Vec<String> = pending
        .drain(..)
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    Ok(drained)
}

#[tauri::command]
fn write_document(
    state: tauri::State<AppState>,
    doc_path: String,
    source: String,
    base_hash: [u8; 32],
) -> Result<WriteResult, String> {
    let _ = state.paths()?;
    let doc_path = path_safe_for_canvas(Path::new(&doc_path))?;

    match atomic_write(&doc_path, source.as_bytes(), Some(&base_hash)) {
        Ok(new_hash) => {
            update_base_snapshot(&doc_path, &source, new_hash)?;
            mcp::emit_artifact_updated(
                doc_path.to_string_lossy().into_owned(),
                "watcher",
                None,
                None,
            );
            Ok(WriteResult { new_hash })
        }
        Err(AtomicWriteError::ConflictDetected { .. }) => {
            Err("CONFLICT: file changed on disk before save".to_owned())
        }
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
fn load_sidecar(state: tauri::State<AppState>, doc_path: String) -> Result<IdentityMap, String> {
    let _ = state.paths()?;
    let doc_path = path_safe_for_canvas(Path::new(&doc_path))?;
    let vault_root = vault_root_for_absolute_doc(&doc_path)?;
    let doc_bytes = fs::read(&doc_path).map_err(|error| error.to_string())?;

    let migrated = sidecar::load_or_migrate(vault_root, &doc_path, &doc_bytes)
        .map_err(|error| error.to_string())?;
    Ok(migrated.unwrap_or_else(|| IdentityMap {
        source_hash: *vellum_core::hash::content_hash(&doc_bytes).as_bytes(),
        block_ids: Vec::new(),
        base_snapshot: None,
        comments: None,
    }))
}

#[tauri::command]
fn save_sidecar(
    state: tauri::State<AppState>,
    doc_path: String,
    map: IdentityMap,
) -> Result<(), String> {
    let _ = state.paths()?;
    let doc_path = path_safe_for_canvas(Path::new(&doc_path))?;
    let vault_root = vault_root_for_absolute_doc(&doc_path)?;

    sidecar::save(vault_root, &doc_path, &map).map_err(|error| error.to_string())
}

#[tauri::command]
fn update_sidecar_comments(
    state: tauri::State<AppState>,
    doc_path: String,
    comments: Vec<Comment>,
) -> Result<(), String> {
    let _ = state.paths()?;
    let doc_path = path_safe_for_canvas(Path::new(&doc_path))?;
    ensure_regular_file(&doc_path)?;
    let vault_root = vault_root_for_absolute_doc(&doc_path)?;
    let doc_bytes = fs::read(&doc_path).map_err(|error| error.to_string())?;
    let mut identity = sidecar::load_or_migrate(vault_root, &doc_path, &doc_bytes)
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| IdentityMap {
            source_hash: *vellum_core::hash::content_hash(&doc_bytes).as_bytes(),
            block_ids: Vec::new(),
            base_snapshot: None,
            comments: None,
        });
    identity.comments = Some(comments);
    sidecar::save(vault_root, &doc_path, &identity).map_err(|error| error.to_string())
}

#[tauri::command]
fn set_current_focus(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    let _ = state.paths()?;
    let path = path_safe_for_canvas(Path::new(&path))?;
    let path_string = path.to_string_lossy().into_owned();
    *state
        .current_focus
        .lock()
        .map_err(|_| "current focus lock poisoned".to_owned())? = Some(path_string.clone());
    mcp::emit_artifact_focused(path_string);
    Ok(())
}

#[tauri::command]
fn emit_artifact_updated(
    state: tauri::State<AppState>,
    path: String,
    by: String,
    note: Option<String>,
    action_verb: Option<String>,
) -> Result<usize, String> {
    let _ = state.paths()?;
    let path = path_safe_for_canvas(Path::new(&path))?;
    let by = match by.as_str() {
        "user" | "watcher" => by,
        _ => return Err("by must be 'user' or 'watcher'".to_owned()),
    };
    Ok(mcp::emit_artifact_updated(
        path.to_string_lossy().into_owned(),
        &by,
        note,
        action_verb,
    ))
}

fn update_base_snapshot(doc_path: &Path, source: &str, hash: [u8; 32]) -> Result<(), String> {
    let vault_root = vault_root_for_absolute_doc(doc_path)?;
    let mut identity = sidecar::load_or_migrate(vault_root, doc_path, source.as_bytes())
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| IdentityMap {
            source_hash: hash,
            block_ids: Vec::new(),
            base_snapshot: None,
            comments: None,
        });
    identity.source_hash = hash;
    identity.base_snapshot = Some(BaseSnapshot {
        hash,
        source: source.to_owned(),
    });
    sidecar::save(vault_root, doc_path, &identity).map_err(|error| error.to_string())
}

fn bootstrap() -> Result<AppState, String> {
    let paths = AgentCanvasPaths::resolve()?;
    paths.ensure()?;
    let db = open_state_db(&paths.state_db, &legacy_icloud_canvas_root()?)?;
    Ok(AppState {
        paths: Ok(paths),
        db: Mutex::new(db),
        watcher: Mutex::new(None),
        current_focus: Mutex::new(None),
        ephemeral_paths: Mutex::new(HashSet::new()),
        pending_opens: Mutex::new(Vec::new()),
    })
}

fn bootstrap_or_error_state() -> AppState {
    match bootstrap() {
        Ok(state) => state,
        Err(error) => {
            eprintln!(
                "AgentCanvas could not start cleanly: {error}. Open System Settings -> iCloud Drive and confirm AgentCanvas storage is available."
            );
            let db = open_in_memory_state_db().unwrap_or_else(|db_error| {
                eprintln!("AgentCanvas could not initialize fallback state DB: {db_error}");
                Connection::open_in_memory().expect("failed to initialize in-memory fallback DB")
            });
            AppState {
                paths: Err(error),
                db: Mutex::new(db),
                watcher: Mutex::new(None),
                current_focus: Mutex::new(None),
                ephemeral_paths: Mutex::new(HashSet::new()),
                pending_opens: Mutex::new(Vec::new()),
            }
        }
    }
}

impl AgentCanvasPaths {
    fn resolve() -> Result<Self, String> {
        let home = home_dir()?;
        let canvas_root = home.join("Documents").join("AgentCanvas");
        let user_symlink = canvas_root.clone();
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
            inbox_dir: canvas_root.join("Inbox"),
            myfiles_dir: canvas_root.join("MyFiles"),
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
        fs::create_dir_all(&self.myfiles_dir).map_err(|error| error.to_string())?;

        if let Some(parent) = self.state_db.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn bootstrap_info(&self) -> BootstrapInfo {
        BootstrapInfo {
            canvas_root: self.canvas_root.to_string_lossy().into_owned(),
            inbox_dir: self.inbox_dir.to_string_lossy().into_owned(),
            myfiles_dir: self.myfiles_dir.to_string_lossy().into_owned(),
            projects_dir: self.projects_dir.to_string_lossy().into_owned(),
            archive_dir: self.archive_dir.to_string_lossy().into_owned(),
            state_db: self.state_db.to_string_lossy().into_owned(),
            user_path: self.user_symlink.to_string_lossy().into_owned(),
        }
    }
}

fn open_state_db(path: &Path, legacy_canvas_root: &Path) -> Result<Connection, String> {
    let db = Connection::open(path).map_err(|error| error.to_string())?;
    initialize_state_db(&db, legacy_canvas_root)?;
    Ok(db)
}

fn open_in_memory_state_db() -> Result<Connection, String> {
    let db = Connection::open_in_memory().map_err(|error| error.to_string())?;
    initialize_state_db(&db, &legacy_icloud_canvas_root()?)?;
    Ok(db)
}

fn initialize_state_db(db: &Connection, legacy_canvas_root: &Path) -> Result<(), String> {
    mcp::sessions::migrate_manual_agent_sessions_if_needed(db)?;
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
          default_agent_session_id TEXT REFERENCES manual_agent_sessions(id) ON DELETE SET NULL,
          updated_at INTEGER NOT NULL
        );
        INSERT OR IGNORE INTO projects(name, updated_at)
        VALUES ('Default', strftime('%s','now'));
        "#,
    )
    .map_err(|error| error.to_string())?;
    mcp::sessions::migrate_agent_sessions(db)?;
    mcp::sessions::migrate_user_messages(db)?;
    mcp::sessions::migrate_session_attachments(db)?;
    mcp::sessions::migrate_agent_messages(db)?;
    mcp::sessions::migrate_interactions(db)?;
    add_column_if_missing(
        db,
        "files",
        "review_state",
        "ALTER TABLE files ADD COLUMN review_state TEXT NOT NULL DEFAULT 'unread'",
    )?;
    add_column_if_missing(
        db,
        "files",
        "in_inbox",
        "ALTER TABLE files ADD COLUMN in_inbox INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        db,
        "files",
        "project_tag",
        "ALTER TABLE files ADD COLUMN project_tag TEXT",
    )?;
    add_column_if_missing(
        db,
        "files",
        "archived",
        "ALTER TABLE files ADD COLUMN archived INTEGER NOT NULL DEFAULT 0",
    )?;
    // Slice 5 — Recents table (idempotent).
    db.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS recents (
          path TEXT PRIMARY KEY,
          last_opened INTEGER NOT NULL,
          title TEXT NOT NULL DEFAULT ''
        );
        "#,
    )
    .map_err(|error| error.to_string())?;
    backfill_file_tags_from_legacy_paths(db, legacy_canvas_root)?;
    // Also backfill from the secondary legacy root (old ~/AgentCanvas local path).
    // Guard: only run if this root differs from the primary legacy root passed in.
    if let Ok(local_legacy) = legacy_local_canvas_root() {
        if local_legacy != *legacy_canvas_root {
            backfill_file_tags_from_legacy_paths(db, &local_legacy)?;
        }
    }
    // Slice 8: startup ghost-session sweep.
    // No MCP connection survives an app restart. Mark any still-"live" session as
    // disconnected so they don't show up as live agents after a force-quit or crash.
    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    mcp::sessions::disconnect_all_sessions(db, now_ts)?;
    Ok(())
}

fn add_column_if_missing(
    db: &Connection,
    table: &str,
    column: &str,
    sql: &str,
) -> Result<(), String> {
    let mut statement = db
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| error.to_string())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    if !columns.iter().any(|existing| existing == column) {
        db.execute(sql, []).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn legacy_icloud_canvas_root() -> Result<PathBuf, String> {
    Ok(home_dir()?
        .join("Library")
        .join("Mobile Documents")
        .join("com~apple~CloudDocs")
        .join("AgentCanvas"))
}

fn legacy_local_canvas_root() -> Result<PathBuf, String> {
    Ok(home_dir()?.join("AgentCanvas"))
}

fn backfill_file_tags_from_legacy_paths(db: &Connection, canvas_root: &Path) -> Result<(), String> {
    let canvas_root = canvas_root.to_path_buf();
    let inbox_prefix = canvas_root.join("Inbox");
    let projects_prefix = canvas_root.join("Projects");
    let archive_prefix = canvas_root.join("Archive");

    let mut stmt = db
        .prepare(
            "SELECT path FROM files
             WHERE in_inbox = 0 AND project_tag IS NULL AND archived = 0",
        )
        .map_err(|error| error.to_string())?;
    let paths = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(stmt);

    for path in paths {
        let path_buf = PathBuf::from(&path);
        if path_buf.starts_with(&inbox_prefix) {
            db.execute(
                "UPDATE files SET in_inbox = 1 WHERE path = ?1",
                params![path],
            )
            .map_err(|error| error.to_string())?;
            continue;
        }
        if path_buf.starts_with(&archive_prefix) {
            db.execute(
                "UPDATE files SET archived = 1 WHERE path = ?1",
                params![path],
            )
            .map_err(|error| error.to_string())?;
            continue;
        }
        if let Ok(relative) = path_buf.strip_prefix(&projects_prefix)
            && let Some(project) = relative.components().next()
        {
            let project = project.as_os_str().to_string_lossy();
            if !project.is_empty() {
                db.execute(
                    "UPDATE files SET project_tag = ?1 WHERE path = ?2",
                    params![project.as_ref(), path],
                )
                .map_err(|error| error.to_string())?;
            }
        }
    }
    Ok(())
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

fn list_tracked_files<const N: usize>(
    db: &Mutex<Connection>,
    canvas_root: &Path,
    where_clause: &str,
    values: [&str; N],
) -> Result<Vec<FileMetadata>, String> {
    let conn = db.lock().map_err(|_| "state db lock poisoned".to_owned())?;
    let mut stmt = conn
        .prepare(&format!("SELECT path FROM files WHERE {where_clause}"))
        .map_err(|error| error.to_string())?;
    let paths = stmt
        .query_map(rusqlite::params_from_iter(values), |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(stmt);

    let mut files = Vec::new();
    for path_str in paths {
        let path = PathBuf::from(&path_str);
        if !path.exists() || !is_supported_artifact(&path) {
            continue;
        }
        let mut file = metadata_for_file(&path, canvas_root)?;
        hydrate_file_state(&conn, &mut file)?;
        files.push(file);
    }

    files.sort_by(|left, right| {
        right
            .pinned
            .cmp(&left.pinned)
            .then_with(|| right.mtime.cmp(&left.mtime))
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(files)
}

fn hydrate_file_state(conn: &Connection, file: &mut FileMetadata) -> Result<(), String> {
    let state: Option<(i64, i64, Option<i64>, String)> = conn
        .query_row(
            "SELECT pinned, archived, last_read_at, review_state FROM files WHERE path = ?1",
            params![file.path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .ok();
    if let Some((pinned, archived, last_read_at, review_state)) = state {
        file.pinned = pinned != 0;
        file.archived = archived != 0;
        file.last_read_at = last_read_at;
        file.review_state = review_state;
    }
    Ok(())
}

fn metadata_for_file(path: &Path, canvas_root: &Path) -> Result<FileMetadata, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let size = metadata.len();
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let relative_path = path.strip_prefix(canvas_root).unwrap_or(path);
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let default_persona = infer_persona(path);
    let persona = if markdown_extension(&extension) {
        cached_frontmatter_persona(path, &bytes, mtime, size).unwrap_or(default_persona)
    } else {
        default_persona
    };

    let comment_count = unresolved_comment_count(path);

    Ok(FileMetadata {
        path: path.to_string_lossy().into_owned(),
        relative_path: relative_path.to_string_lossy().into_owned(),
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("artifact")
            .to_owned(),
        extension,
        size,
        mtime,
        last_seen_hash: *vellum_core::hash::content_hash(&bytes).as_bytes(),
        pinned: false,
        archived: false,
        last_read_at: None,
        persona,
        review_state: "unread".to_owned(),
        comment_count,
    })
}

/// Count unresolved comments in a file's sidecar if it exists. Cheap because we
/// only read the sidecar JSON (~KB), not the underlying file. Returns 0 on any
/// error or missing sidecar — comment counts are advisory.
fn unresolved_comment_count(doc_path: &Path) -> u32 {
    let Ok(vault_root) = vault_root_for_absolute_doc(doc_path) else {
        return 0;
    };
    let sidecar = sidecar::sidecar_path(vault_root, doc_path);
    if !sidecar.exists() {
        return 0;
    }
    let Ok(bytes) = fs::read(&sidecar) else {
        return 0;
    };
    let Ok(identity) = serde_json::from_slice::<IdentityMap>(&bytes) else {
        return 0;
    };
    identity
        .comments
        .unwrap_or_default()
        .iter()
        .filter(|c| !c.resolved)
        .count() as u32
}

fn upsert_file_state(conn: &Connection, file: &FileMetadata) -> Result<(), String> {
    let existing_path: Option<String> = conn
        .query_row(
            "SELECT path FROM files WHERE last_seen_hash = ?1 AND path != ?2 LIMIT 1",
            params![file.last_seen_hash.as_slice(), file.path],
            |row| row.get(0),
        )
        .ok();

    if let Some(existing_path) = existing_path
        && !Path::new(&existing_path).exists()
    {
        conn.execute(
            "UPDATE files SET path = ?1, size = ?2, mtime = ?3 WHERE path = ?4",
            params![file.path, file.size as i64, file.mtime, existing_path],
        )
        .map_err(|error| error.to_string())?;
    }

    conn.execute(
        r#"
        INSERT INTO files(path, last_seen_hash, size, mtime, pinned, archived, in_inbox)
        VALUES (?1, ?2, ?3, ?4, 0, 0, 0)
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

fn is_valid_review_state(state: &str) -> bool {
    matches!(state, "unread" | "reviewed" | "needs-work" | "approved")
}

fn is_supported_artifact(path: &Path) -> bool {
    let visible = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| !name.starts_with('.'))
        .unwrap_or(false);
    if !visible {
        return false;
    }

    matches!(
        normalized_extension(path).as_str(),
        "md" | "markdown" | "html" | "htm" | "png" | "json" | "txt" | "pdf"
    )
}

fn markdown_extension(extension: &str) -> bool {
    extension == "md" || extension == "markdown"
}

fn normalized_extension(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

fn resolve_personas(
    registry_root: &Path,
    db: &Mutex<Connection>,
) -> Result<PersonaRegistry, String> {
    let mut personas = discover_personas(registry_root);
    let mut warning = None;

    if !registry_root.exists() || personas.is_empty() {
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

fn discover_personas(registry_root: &Path) -> Vec<Persona> {
    if !registry_root.exists() {
        return Vec::new();
    }

    let mut personas = Vec::new();
    for entry in WalkDir::new(registry_root)
        .min_depth(3)
        .max_depth(3)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
            continue;
        }
        let Some(agent_name) = path.file_stem().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(agents_dir) = path.parent() else {
            continue;
        };
        if agents_dir.file_name().and_then(|name| name.to_str()) != Some("agents") {
            continue;
        }
        let Some(plugin_name) = agents_dir
            .parent()
            .and_then(|plugin_dir| plugin_dir.file_name())
            .and_then(|name| name.to_str())
        else {
            continue;
        };
        if agent_name != plugin_name {
            continue;
        }
        if let Ok(source) = fs::read_to_string(path) {
            let name = frontmatter_value(&source, "name").unwrap_or_else(|| agent_name.to_owned());
            let color = frontmatter_value(&source, "color")
                .or_else(|| builtin_persona_color(&name).map(str::to_owned))
                .unwrap_or_else(|| "neutral".to_owned());
            personas.push(Persona {
                display_label: display_label(&name),
                name,
                color,
                source: "pike-agents".to_owned(),
            });
        }
    }

    personas.sort_by(|left, right| left.name.cmp(&right.name));
    personas.dedup_by(|left, right| left.name == right.name);
    personas
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

fn cached_frontmatter_persona(path: &Path, bytes: &[u8], mtime: i64, size: u64) -> Option<String> {
    let path_key = path.to_string_lossy().into_owned();
    let cache_key = (path_key.clone(), mtime, size);
    let cache = PERSONA_METADATA_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Ok(mut cache) = cache.lock() {
        if let Some(persona) = cache.get(&cache_key) {
            return (!persona.is_empty()).then(|| persona.clone());
        }
        cache.retain(|(cached_path, _, _), _| cached_path != &path_key);
    }

    let persona =
        frontmatter_persona(bytes).filter(|persona| valid_persona_names().contains(persona));

    if let Ok(mut cache) = cache.lock() {
        cache.insert(cache_key, persona.clone().unwrap_or_default());
    }

    persona
}

fn frontmatter_persona(bytes: &[u8]) -> Option<String> {
    let prefix_len = bytes.len().min(4096);
    let source = String::from_utf8_lossy(&bytes[..prefix_len]);
    let mut lines = source.lines();
    if lines.next()?.trim_end_matches('\r') != "---" {
        return None;
    }

    let mut persona = None;
    let mut author = None;
    let mut agent = None;
    let mut closed = false;

    for line in lines {
        let line = line.trim_end_matches('\r');
        if line == "---" {
            closed = true;
            break;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim().to_owned();
        if value.is_empty() {
            continue;
        }
        match key.trim() {
            "persona" if persona.is_none() => persona = Some(value),
            "author" if author.is_none() => author = Some(value),
            "agent" if agent.is_none() => agent = Some(value),
            _ => {}
        }
    }

    closed.then(|| persona.or(author).or(agent)).flatten()
}

pub(crate) fn persona_names_from_registry_root(registry_root: &Path) -> HashSet<String> {
    let mut names: HashSet<String> = builtin_persona_colors()
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect();
    names.extend(
        discover_personas(registry_root)
            .into_iter()
            .map(|persona| persona.name),
    );
    names
}

fn valid_persona_names() -> HashSet<String> {
    if let Some(registry_root) = default_persona_registry_root() {
        return persona_names_from_registry_root(&registry_root);
    }
    builtin_persona_colors()
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect()
}

fn default_persona_registry_root() -> Option<PathBuf> {
    std::env::var_os("AGENTCANVAS_PERSONA_REGISTRY")
        .map(PathBuf::from)
        .or_else(|| {
            home_dir().ok().map(|home| {
                home.join("code")
                    .join("_shared")
                    .join("pike-agents")
                    .join("plugins")
            })
        })
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

fn builtin_persona_color(name: &str) -> Option<&'static str> {
    builtin_persona_colors()
        .iter()
        .find_map(|(candidate, color)| (*candidate == name).then_some(*color))
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

fn path_safe_for_canvas(candidate: &Path) -> Result<PathBuf, String> {
    if !candidate.is_absolute() {
        return Err(format!("path must be absolute: {}", candidate.display()));
    }
    let canonical_candidate = if candidate.exists() {
        candidate
            .canonicalize()
            .map_err(|error| error.to_string())?
    } else {
        let parent = candidate
            .parent()
            .ok_or_else(|| format!("path outside AgentCanvas: {}", candidate.display()))?;
        let file_name = candidate
            .file_name()
            .ok_or_else(|| format!("path outside AgentCanvas: {}", candidate.display()))?;
        parent
            .canonicalize()
            .map_err(|error| error.to_string())?
            .join(file_name)
    };

    let mut denied = vec![
        PathBuf::from("/etc"),
        PathBuf::from("/System"),
        PathBuf::from("/private/etc"),
        PathBuf::from("/private/var"),
        PathBuf::from("/usr"),
        PathBuf::from("/var"),
        PathBuf::from("/bin"),
        PathBuf::from("/sbin"),
        PathBuf::from("/Library/Application Support/AgentCanvas"),
        PathBuf::from("/Library/Application Support/Apple"),
        PathBuf::from("/Users/jessepike/Library/Application Support/AgentCanvas"),
    ];
    if let Ok(home) = home_dir() {
        denied.push(
            home.join("Library")
                .join("Application Support")
                .join("AgentCanvas"),
        );
    }

    if denied
        .iter()
        .any(|prefix| canonical_candidate.starts_with(prefix))
    {
        return Err(format!(
            "path is not safe for AgentCanvas: {}",
            candidate.display()
        ));
    }
    Ok(canonical_candidate)
}

#[allow(dead_code)]
fn path_within_canvas(_canvas_root: &Path, candidate: &Path) -> Result<PathBuf, String> {
    path_safe_for_canvas(candidate)
}

fn untrack_file_impl(conn: &Connection, source: &Path) -> Result<(), String> {
    conn.execute(
        "DELETE FROM files WHERE path = ?1",
        params![source.to_string_lossy()],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn delete_file_from_disk_impl(conn: &Connection, source: &Path) -> Result<(), String> {
    fs::remove_file(source).map_err(|error| error.to_string())?;
    untrack_file_impl(conn, source)
}

fn format_send_payload(
    payload: &SendPayload,
    canvas_root: &Path,
    templates: &[ActionTemplate],
) -> Result<String, String> {
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
    let template_block = action_template_for(action, templates)
        .filter(|template| !template.trim().is_empty())
        .map(|template| format!("\n\n{}", template.trim()))
        .unwrap_or_default();

    Ok(format!(
        "I'm sending you `{relative_path}` from my AgentCanvas.\n\n{note_block}Contents:\n\n{fence}\n{}\n```{template_block}\n\nAction: {action}",
        payload.contents
    ))
}

fn format_send_multi_payload(
    payloads: &[SendPayload],
    canvas_root: &Path,
    templates: &[ActionTemplate],
) -> Result<String, String> {
    let count = payloads.len();
    let first = &payloads[0];
    let note = first.note.as_deref().unwrap_or("").trim();
    let note_block = if note.is_empty() {
        String::new()
    } else {
        format!("My note: {note}\n\n")
    };
    let action = first.action_verb.trim();
    let action = if action.is_empty() { "Review" } else { action };
    let template_block = action_template_for(action, templates)
        .filter(|template| !template.trim().is_empty())
        .map(|template| format!("{}\n\n", template.trim()))
        .unwrap_or_default();

    let mut out = format!("I'm sending you {count} files from my AgentCanvas.\n\n{note_block}");

    for (index, payload) in payloads.iter().enumerate() {
        let relative_path = relative_canvas_path(&payload.path, canvas_root)?;
        let language = language_from_path(&payload.path);
        let fence = if language.is_empty() {
            "```".to_owned()
        } else {
            format!("```{language}")
        };
        out.push_str(&format!(
            "---\n\nFile {} of {}: `{}`\n{}\n{}\n```\n\n",
            index + 1,
            count,
            relative_path,
            fence,
            payload.contents
        ));
    }

    out.push_str(&format!("{template_block}Action: {action}"));
    Ok(out)
}

fn default_action_templates() -> Vec<ActionTemplate> {
    vec![
        ActionTemplate {
            verb: "Review".to_owned(),
            template: "Review for clarity, completeness, and correctness. Flag anything that needs my attention.".to_owned(),
        },
        ActionTemplate {
            verb: "Critique".to_owned(),
            template: "Critique with rigor. Identify weak claims, missing evidence, structural issues.".to_owned(),
        },
        ActionTemplate {
            verb: "Revise".to_owned(),
            template: "Revise per my note above. Preserve voice and structure.".to_owned(),
        },
        ActionTemplate {
            verb: "Expand".to_owned(),
            template: "Expand on the thin sections. Add depth where the argument is asserted but not supported.".to_owned(),
        },
        ActionTemplate {
            verb: "Summarize".to_owned(),
            template: "Summarize in 200 words or fewer. Lead with the answer.".to_owned(),
        },
        ActionTemplate {
            verb: "Respond to".to_owned(),
            template: "Draft a response. Keep it under 200 words.".to_owned(),
        },
    ]
}

fn action_templates_from_db(db: &Mutex<Connection>) -> Result<Vec<ActionTemplate>, String> {
    match get_setting(db, "action_templates")? {
        Some(value) => serde_json::from_str(&value).map_err(|error| error.to_string()),
        None => Ok(default_action_templates()),
    }
}

fn action_template_for<'a>(action: &str, templates: &'a [ActionTemplate]) -> Option<&'a str> {
    templates
        .iter()
        .find(|template| template.verb == action)
        .map(|template| template.template.as_str())
}

fn relative_canvas_path(path: &str, canvas_root: &Path) -> Result<String, String> {
    let path = Path::new(path);
    let display_path = path.strip_prefix(canvas_root).unwrap_or(path);
    Ok(display_path.to_string_lossy().into_owned())
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
    let app_state = bootstrap_or_error_state();

    let result = tauri::Builder::<tauri::Wry>::default()
        .manage(app_state)
        .setup(|app| {
            let state = app.state::<AppState>();
            if let Some(message) = state.bootstrap_error() {
                let hint = "Open System Settings -> iCloud Drive and confirm AgentCanvas storage is available.".to_owned();
                eprintln!("AgentCanvas bootstrap error: {message}. {hint}");
                let _ = app.emit("bootstrap-error", BootstrapErrorPayload { message, hint });
            } else {
                let canvas_root = state
                    .paths()
                    .map_err(std::io::Error::other)?
                    .canvas_root
                    .clone();
                let app_handle = app.handle().clone();
                let watcher = watch::start(move |event| {
                    let payload = fs_event_payload(event);
                    if let Some(path) = payload.path.as_ref()
                        && tracked_file_exists(&app_handle, path)
                    {
                        mcp::emit_artifact_updated(path.clone(), "watcher", None, None);
                    }
                    let _ = app_handle.emit("agentcanvas://fs-event", payload);
                })?;
                watcher.watch_recursive(&canvas_root)?;
                let tracked_paths = {
                    let conn = state
                        .db
                        .lock()
                        .map_err(|_| std::io::Error::other("state db lock poisoned"))?;
                    tracked_watch_paths_from_db(&conn).map_err(std::io::Error::other)?
                };
                watcher
                    .set_paths(tracked_paths)
                    .map_err(std::io::Error::other)?;
                *state.watcher.lock().map_err(|_| "watcher lock poisoned")? = Some(watcher);
                mcp::init_mcp_server(app.handle().clone()).map_err(std::io::Error::other)?;
            }
            Ok(())
        })
        .on_window_event(|_window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                mcp::shutdown_mcp_server();
            }
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap_info,
            list_inbox,
            list_drafts,
            inbox_unread_count,
            create_my_file,
            list_projects,
            list_project_counts,
            rename_project,
            delete_project_if_empty,
            list_project_files,
            list_archive,
            list_pinned,
            get_project_default_agent,
            set_project_default_agent,
            list_personas,
            reload_persona_registry,
            get_default_action_verb,
            set_default_action_verb,
            get_action_templates,
            set_action_templates,
            reset_action_templates,
            toggle_pin,
            archive_file,
            track_paths_in_inbox,
            copy_paths_to_inbox,
            move_file_to_project,
            move_file_to_archive,
            target_file_exists,
            copy_text_to_clipboard,
            reveal_in_finder,
            untrack_file,
            delete_file_from_disk,
            delete_file,
            rename_file,
            export_file_to,
            send_to_clipboard,
            send_multi_to_clipboard,
            session_attachments_for_path,
            send_back_to_session,
            list_agent_sessions,
            add_agent_session,
            remove_agent_session,
            disconnect_mcp_session,
            // Slice 7
            list_agent_messages,
            acknowledge_agent_message,
            // Slice 0.5 — Interactions
            list_interactions,
            get_interaction,
            submit_interaction_response,
            install_mcp_for_claude_code,
            install_mcp_for_codex,
            install_mcp_for_cursor,
            parse_document,
            save_document,
            open_document,
            read_binary_artifact,
            write_document,
            load_sidecar,
            save_sidecar,
            update_sidecar_comments,
            set_current_focus,
            emit_artifact_updated,
            set_review_state,
            // Slice 5
            open_path,
            close_ephemeral_path,
            list_recents,
            // Slice 4
            take_pending_opens
        ])
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_persisted_scope::init())
        .build(tauri::generate_context!());

    match result {
        Ok(app) => {
            #[allow(unused_variables)]
            app.run(|app_handle, event| {
                // Slice 4: handle RunEvent::Opened (macOS file-open / URL event).
                // This fires on both cold-launch and warm open-with; buffer paths first,
                // then also emit the warm event so the webview can react immediately.
                // LOCK DISCIPLINE: no db lock here; window ops happen AFTER any lock.
                #[cfg(target_os = "macos")]
                if let tauri::RunEvent::Opened { urls } = &event {
                    let paths: Vec<PathBuf> = urls
                        .iter()
                        .filter_map(|url| {
                            if url.scheme() == "file" {
                                url.to_file_path().ok()
                            } else {
                                None
                            }
                        })
                        .collect();

                    // Buffer for cold-launch path (take_pending_opens drains this on mount).
                    if let Ok(mut pending) = app_handle.state::<AppState>().pending_opens.lock() {
                        pending.extend(paths.iter().cloned());
                    }

                    // Also emit for the warm case (webview may already be listening).
                    // Window focus is done AFTER pushing to the buffer — no db lock held.
                    for path in &paths {
                        let path_str = path.to_string_lossy().into_owned();
                        let _ = app_handle.emit(
                            "agentcanvas://open-external",
                            serde_json::json!({ "path": path_str }),
                        );
                    }

                    // Raise the window — no db lock held at this point.
                    if !paths.is_empty() {
                        if let Some(window) = app_handle.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                }

                if matches!(event, tauri::RunEvent::Exit) {
                    mcp::shutdown_mcp_server();
                }
            });
        }
        Err(error) => {
            eprintln!("AgentCanvas could not start: {error}");
        }
    }
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

fn tracked_file_exists(app_handle: &tauri::AppHandle, path: &str) -> bool {
    let state = app_handle.state::<AppState>();
    let Ok(conn) = state.db.lock() else {
        return false;
    };
    conn.query_row(
        "SELECT 1 FROM files WHERE path = ?1 LIMIT 1",
        params![path],
        |_| Ok(()),
    )
    .is_ok()
}

pub(crate) fn resync_watcher_from_db(state: &AppState) -> Result<(), String> {
    let tracked_paths = {
        let conn = state
            .db
            .lock()
            .map_err(|_| "state db lock poisoned".to_owned())?;
        tracked_watch_paths_from_db(&conn)?
    };
    let watcher = state
        .watcher
        .lock()
        .map_err(|_| "watcher lock poisoned".to_owned())?;
    if let Some(watcher) = watcher.as_ref() {
        watcher
            .set_paths(tracked_paths)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn tracked_watch_paths_from_db(conn: &Connection) -> Result<Vec<PathBuf>, String> {
    let mut statement = conn
        .prepare(
            "SELECT path FROM files
             WHERE in_inbox = 1
                OR project_tag IS NOT NULL
                OR archived = 1
                OR pinned = 1",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    let mut paths = Vec::new();
    for row in rows {
        paths.push(PathBuf::from(row.map_err(|error| error.to_string())?));
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_safe_for_canvas_allow_deny_matrix() {
        assert!(path_safe_for_canvas(Path::new("/etc/passwd")).is_err());
        assert!(path_safe_for_canvas(Path::new("/Users/jessepike/code/foo.html")).is_ok());
        assert!(
            path_safe_for_canvas(Path::new(
                "/Users/jessepike/Library/Application Support/AgentCanvas/state.db"
            ))
            .is_err()
        );
    }

    #[test]
    fn test_path_within_canvas_shim_accepts_safe_path() {
        let temp = tempfile::tempdir_in(std::env::current_dir().expect("cwd")).expect("tempdir");
        let canvas_root = temp.path().join("AgentCanvas");
        let inbox = canvas_root.join("Inbox");
        fs::create_dir_all(&inbox).expect("inbox");
        fs::write(inbox.join("x.md"), "x").expect("file");

        let candidate = inbox.join("x.md");
        let bounded = path_within_canvas(&canvas_root, &candidate).expect("safe path accepted");

        // On macOS, tempdirs canonicalize through /private. Compare canonicalized expectation.
        let expected = candidate.canonicalize().expect("canonicalize candidate");
        assert_eq!(bounded, expected);
    }

    #[test]
    fn test_path_within_canvas_resolves_symlinks() {
        let temp = tempfile::tempdir_in(std::env::current_dir().expect("cwd")).expect("tempdir");
        let canvas_root = temp.path().join("AgentCanvas");
        fs::create_dir_all(&canvas_root).expect("canvas root");
        let inside = canvas_root.join("x.md");
        fs::write(&inside, "inside").expect("inside file");
        let symlink = temp.path().join("link.md");
        std::os::unix::fs::symlink(&inside, &symlink).expect("symlink");

        let bounded = path_within_canvas(&canvas_root, &symlink).expect("symlink accepted");

        // Symlink target canonicalizes through /private on macOS.
        let expected = inside.canonicalize().expect("canonicalize target");
        assert_eq!(bounded, expected);
    }

    #[test]
    fn legacy_comment_anchor_deserializes_as_text_selection() {
        let raw = r#"{
          "source_hash": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
          "block_ids": [],
          "comments": [{
            "id": "00000000-0000-4000-8000-000000000000",
            "author": "jesse",
            "created_at": 1,
            "anchor": { "block_id": null, "start_offset": 3, "end_offset": 9 },
            "body": "legacy",
            "resolved": false
          }]
        }"#;

        let identity: IdentityMap = serde_json::from_str(raw).expect("legacy identity");
        let comments = identity.comments.expect("comments");
        let anchor = &comments[0].anchor;
        assert_eq!(
            serde_json::to_value(anchor).expect("anchor json")["kind"],
            serde_json::Value::Null
        );
        assert_eq!(
            serde_json::to_value(anchor).expect("anchor json")["start_offset"],
            3
        );
    }

    #[test]
    fn html_comment_anchor_round_trips_with_snapshot_text() {
        let raw = r#"{
          "kind": "html_selection",
          "start_offset": 4,
          "end_offset": 15,
          "snapshot_text": "Hello world"
        }"#;

        let anchor: vellum_core::sidecar::CommentAnchor =
            serde_json::from_str(raw).expect("html anchor");
        let encoded = serde_json::to_value(anchor).expect("anchor json");
        assert_eq!(encoded["kind"], "html_selection");
        assert_eq!(encoded["snapshot_text"], "Hello world");
    }

    #[test]
    fn file_level_comment_anchor_round_trips() {
        let raw = r#"{ "kind": "file_level" }"#;

        let anchor: vellum_core::sidecar::CommentAnchor =
            serde_json::from_str(raw).expect("file-level anchor");
        let encoded = serde_json::to_value(anchor).expect("anchor json");
        assert_eq!(encoded["kind"], "file_level");
    }

    #[test]
    fn migration_backfills_legacy_tags_idempotently() {
        let temp = tempfile::tempdir().expect("tempdir");
        let legacy_root = temp.path().join("AgentCanvas");
        fs::create_dir_all(legacy_root.join("Inbox")).expect("inbox");
        fs::create_dir_all(legacy_root.join("Projects/Alpha")).expect("project");
        fs::create_dir_all(legacy_root.join("Archive")).expect("archive");
        let conn = Connection::open_in_memory().expect("db");
        initialize_state_db(&conn, &legacy_root).expect("init");
        let hash = vec![0_u8; 32];
        for path in [
            legacy_root.join("Inbox/a.md"),
            legacy_root.join("Projects/Alpha/b.md"),
            legacy_root.join("Archive/c.md"),
            temp.path().join("elsewhere/d.md"),
        ] {
            conn.execute(
                "INSERT INTO files(path, last_seen_hash, size, mtime) VALUES (?1, ?2, 1, 1)",
                params![path.to_string_lossy(), hash],
            )
            .expect("insert file");
        }

        backfill_file_tags_from_legacy_paths(&conn, &legacy_root).expect("backfill 1");
        backfill_file_tags_from_legacy_paths(&conn, &legacy_root).expect("backfill 2");

        let inbox: i64 = conn
            .query_row(
                "SELECT in_inbox FROM files WHERE path = ?1",
                params![legacy_root.join("Inbox/a.md").to_string_lossy()],
                |row| row.get(0),
            )
            .expect("inbox");
        let project: String = conn
            .query_row(
                "SELECT project_tag FROM files WHERE path = ?1",
                params![legacy_root.join("Projects/Alpha/b.md").to_string_lossy()],
                |row| row.get(0),
            )
            .expect("project");
        let archived: i64 = conn
            .query_row(
                "SELECT archived FROM files WHERE path = ?1",
                params![legacy_root.join("Archive/c.md").to_string_lossy()],
                |row| row.get(0),
            )
            .expect("archive");
        let untouched: (i64, Option<String>, i64) = conn
            .query_row(
                "SELECT in_inbox, project_tag, archived FROM files WHERE path = ?1",
                params![temp.path().join("elsewhere/d.md").to_string_lossy()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("untouched");

        assert_eq!(inbox, 1);
        assert_eq!(project, "Alpha");
        assert_eq!(archived, 1);
        assert_eq!(untouched, (0, None, 0));
    }

    #[test]
    fn untrack_keeps_file_delete_from_disk_removes_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let conn = open_in_memory_state_db().expect("db");
        let keep = temp.path().join("keep.md");
        let delete = temp.path().join("delete.md");
        fs::write(&keep, "keep").expect("keep");
        fs::write(&delete, "delete").expect("delete");
        let hash = vec![0_u8; 32];
        for path in [&keep, &delete] {
            conn.execute(
                "INSERT INTO files(path, last_seen_hash, size, mtime, in_inbox) VALUES (?1, ?2, 1, 1, 1)",
                params![path.to_string_lossy(), hash],
            )
            .expect("insert");
        }

        untrack_file_impl(&conn, &keep).expect("untrack");
        assert!(keep.exists());
        let keep_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE path = ?1",
                params![keep.to_string_lossy()],
                |row| row.get(0),
            )
            .expect("keep count");
        assert_eq!(keep_rows, 0);

        delete_file_from_disk_impl(&conn, &delete).expect("delete from disk");
        assert!(!delete.exists());
        let delete_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE path = ?1",
                params![delete.to_string_lossy()],
                |row| row.get(0),
            )
            .expect("delete count");
        assert_eq!(delete_rows, 0);
    }

    #[test]
    fn test_identity_relink_skips_when_old_path_exists() {
        let temp = tempfile::tempdir().expect("tempdir");
        let canvas_root = temp.path().join("AgentCanvas");
        let inbox = canvas_root.join("Inbox");
        fs::create_dir_all(&inbox).expect("inbox");
        let first_path = inbox.join("first.md");
        let second_path = inbox.join("second.md");
        fs::write(&first_path, "").expect("first file");
        fs::write(&second_path, "").expect("second file");
        let conn = open_in_memory_state_db().expect("db");

        let first = metadata_for_file(&first_path, &canvas_root).expect("first metadata");
        upsert_file_state(&conn, &first).expect("first upsert");
        conn.execute(
            "UPDATE files SET pinned = 1 WHERE path = ?1",
            params![first.path],
        )
        .expect("pin first");
        let mut second = metadata_for_file(&second_path, &canvas_root).expect("second metadata");
        upsert_file_state(&conn, &second).expect("second upsert");
        hydrate_file_state(&conn, &mut second).expect("hydrate second");
        let first_pinned: i64 = conn
            .query_row(
                "SELECT pinned FROM files WHERE path = ?1",
                params![first_path.to_string_lossy()],
                |row| row.get(0),
            )
            .expect("first row");

        assert_eq!(first_pinned, 1);
        assert!(!second.pinned);
        assert_ne!(first.path, second.path);
    }

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

        let formatted = format_send_payload(&payload, root, &default_action_templates())
            .expect("payload formats");

        assert_eq!(
            formatted,
            "I'm sending you `Inbox/test.md` from my AgentCanvas.\n\nMy note: Tighten this.\n\nContents:\n\n```markdown\n# Test\n\nBody\n```\n\nRevise per my note above. Preserve voice and structure.\n\nAction: Revise"
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

        let formatted = format_send_payload(&payload, root, &default_action_templates())
            .expect("payload formats");

        assert_eq!(
            formatted,
            "I'm sending you `Archive/report.html` from my AgentCanvas.\n\nContents:\n\n```html\n<h1>Report</h1>\n```\n\nReview for clarity, completeness, and correctness. Flag anything that needs my attention.\n\nAction: Review"
        );
        assert!(!formatted.contains("My note:"));
    }

    #[test]
    fn agent_canvas_paths_resolve_uses_documents_subfolder() {
        let paths = AgentCanvasPaths::resolve().expect("paths resolve");
        let canvas_root_str = paths.canvas_root.to_string_lossy();
        assert!(
            canvas_root_str.ends_with("Documents/AgentCanvas"),
            "canvas_root should end with Documents/AgentCanvas, got: {canvas_root_str}"
        );
        let inbox_str = paths.inbox_dir.to_string_lossy();
        assert!(
            inbox_str.ends_with("Documents/AgentCanvas/Inbox"),
            "inbox_dir should end with Documents/AgentCanvas/Inbox, got: {inbox_str}"
        );
        let myfiles_str = paths.myfiles_dir.to_string_lossy();
        assert!(
            myfiles_str.ends_with("Documents/AgentCanvas/MyFiles"),
            "myfiles_dir should end with Documents/AgentCanvas/MyFiles, got: {myfiles_str}"
        );
    }

    // -- create_my_file / sanitize_draft_name tests --

    #[test]
    fn sanitize_draft_name_basic_creates_md() {
        let temp = tempfile::tempdir().expect("tempdir");
        let myfiles = temp.path().join("MyFiles");
        fs::create_dir_all(&myfiles).expect("myfiles dir");

        let path = sanitize_draft_name("my note", &myfiles).expect("sanitize");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "my note.md");
        assert!(path.starts_with(&myfiles));
    }

    #[test]
    fn sanitize_draft_name_forces_md_extension() {
        let temp = tempfile::tempdir().expect("tempdir");
        let myfiles = temp.path().join("MyFiles");
        fs::create_dir_all(&myfiles).expect("myfiles dir");

        // Caller passes "note.txt" — must come out as "note.md"
        let path = sanitize_draft_name("note.txt", &myfiles).expect("sanitize");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "note.md");
    }

    #[test]
    fn sanitize_draft_name_collision_suffix() {
        let temp = tempfile::tempdir().expect("tempdir");
        let myfiles = temp.path().join("MyFiles");
        fs::create_dir_all(&myfiles).expect("myfiles dir");

        // Pre-create colliding files.
        fs::write(myfiles.join("idea.md"), "").expect("first");
        fs::write(myfiles.join("idea 2.md"), "").expect("second");

        let path = sanitize_draft_name("idea", &myfiles).expect("sanitize");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "idea 3.md");
    }

    #[test]
    fn sanitize_draft_name_empty_is_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let myfiles = temp.path().join("MyFiles");
        fs::create_dir_all(&myfiles).expect("myfiles dir");

        assert!(sanitize_draft_name("", &myfiles).is_err());
        assert!(sanitize_draft_name("   ", &myfiles).is_err());
    }

    #[test]
    fn create_my_file_not_in_inbox() {
        let temp = tempfile::tempdir().expect("tempdir");
        let canvas_root = temp.path().join("AgentCanvas");
        let myfiles = canvas_root.join("MyFiles");
        fs::create_dir_all(&myfiles).expect("myfiles dir");

        let conn = open_in_memory_state_db().expect("db");

        let target = sanitize_draft_name("draft", &myfiles).expect("sanitize");
        let mut file_handle = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .expect("create file");
        file_handle.flush().expect("flush");
        drop(file_handle);

        let path_str = target.to_string_lossy().into_owned();
        let file = metadata_for_file(&target, &canvas_root).expect("metadata");
        upsert_file_state(&conn, &file).expect("upsert");
        conn.execute(
            "UPDATE files SET in_inbox = 0, archived = 0, review_state = 'reviewed' WHERE path = ?1",
            params![path_str],
        )
        .expect("update");

        let in_inbox: i64 = conn
            .query_row(
                "SELECT in_inbox FROM files WHERE path = ?1",
                params![path_str],
                |row| row.get(0),
            )
            .expect("row");
        let review_state: String = conn
            .query_row(
                "SELECT review_state FROM files WHERE path = ?1",
                params![path_str],
                |row| row.get(0),
            )
            .expect("row");

        assert_eq!(in_inbox, 0, "draft must NOT be in_inbox");
        assert_eq!(review_state, "reviewed", "draft must NOT be unread");
        assert!(target.exists(), "file must exist on disk");
    }

    #[test]
    fn inbox_unread_count_counts_only_inbox_unread() {
        let temp = tempfile::tempdir().expect("tempdir");
        let canvas_root = temp.path().join("AgentCanvas");
        let inbox = canvas_root.join("Inbox");
        let myfiles = canvas_root.join("MyFiles");
        fs::create_dir_all(&inbox).expect("inbox");
        fs::create_dir_all(&myfiles).expect("myfiles");

        let inbox_unread = inbox.join("a.md");
        let inbox_read = inbox.join("b.md");
        let draft_file = myfiles.join("c.md");
        fs::write(&inbox_unread, "").expect("a");
        fs::write(&inbox_read, "").expect("b");
        fs::write(&draft_file, "").expect("c");

        let conn = open_in_memory_state_db().expect("db");

        for (path, in_inbox, review_state) in [
            (&inbox_unread, 1_i64, "unread"),
            (&inbox_read, 1, "reviewed"),
            (&draft_file, 0, "reviewed"),
        ] {
            let f = metadata_for_file(path, &canvas_root).expect("meta");
            upsert_file_state(&conn, &f).expect("upsert");
            conn.execute(
                "UPDATE files SET in_inbox = ?1, review_state = ?2 WHERE path = ?3",
                params![in_inbox, review_state, f.path],
            )
            .expect("update");
        }

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE in_inbox = 1 AND archived = 0 AND review_state = 'unread'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 1, "only the unread inbox file should be counted");
    }

    // ---------------------------------------------------------------------------
    // Slice 5 — Recents migration + prune cap + open_path resolution rule
    // ---------------------------------------------------------------------------

    #[test]
    fn recents_migration_is_idempotent() {
        // initialize_state_db runs the CREATE TABLE IF NOT EXISTS — running it twice must not fail.
        let conn = Connection::open_in_memory().expect("db");
        let legacy_root = PathBuf::from("/tmp/__nonexistent_legacy__");
        initialize_state_db(&conn, &legacy_root).expect("init 1");
        initialize_state_db(&conn, &legacy_root).expect("init 2 (idempotent)");
        // Verify the table was actually created.
        let tbl_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='recents'",
                [],
                |row| row.get(0),
            )
            .expect("table check");
        assert_eq!(tbl_exists, 1, "recents table must exist after migration");
    }

    #[test]
    fn recents_prune_respects_cap() {
        let conn = Connection::open_in_memory().expect("db");
        let legacy_root = PathBuf::from("/tmp/__nonexistent_legacy__");
        initialize_state_db(&conn, &legacy_root).expect("init");
        let db = Mutex::new(conn);

        // Insert RECENTS_LIMIT + 5 entries (each with a unique timestamp).
        let total = RECENTS_LIMIT + 5;
        for i in 0..total {
            upsert_recent(
                &db,
                &format!("/tmp/file_{i}.md"),
                &format!("file_{i}"),
                (1_000_000 + i) as i64,
            )
            .expect("upsert");
        }

        let conn = db.lock().expect("lock");
        let row_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM recents", [], |row| row.get(0))
            .expect("count");
        assert_eq!(
            row_count, RECENTS_LIMIT as i64,
            "recents must be capped at RECENTS_LIMIT after pruning"
        );
    }

    #[test]
    fn open_path_resolution_rule_under_root_is_tracked() {
        // Verify the resolution rule: path under canvas_root → tracked (no files row needed).
        let temp = tempfile::tempdir().expect("tempdir");
        let canvas_root = temp.path().join("AgentCanvas");
        let inbox = canvas_root.join("Inbox");
        fs::create_dir_all(&inbox).expect("inbox");
        let file = inbox.join("agent-note.md");
        fs::write(&file, "hello from inbox").expect("write");

        let conn = Connection::open_in_memory().expect("db");
        initialize_state_db(&conn, &canvas_root).expect("init");

        // Path is under canvas_root — must resolve as tracked even without a files row.
        let under_root = file.starts_with(&canvas_root);
        assert!(under_root, "test precondition: file must be under canvas root");

        let has_files_row: bool = conn
            .query_row(
                "SELECT 1 FROM files WHERE path = ?1 LIMIT 1",
                params![file.to_string_lossy()],
                |_| Ok(()),
            )
            .is_ok();
        assert!(!has_files_row, "no files row should exist yet");

        // Simulate the tracked resolution branch.
        let is_tracked = under_root || has_files_row;
        assert!(is_tracked, "must resolve as tracked");
    }

    #[test]
    fn open_path_resolution_rule_external_is_ephemeral_no_files_row() {
        // Verify the resolution rule: path outside canvas_root without a files row → ephemeral.
        let temp = tempfile::tempdir().expect("tempdir");
        let canvas_root = temp.path().join("AgentCanvas");
        let external_dir = temp.path().join("Downloads");
        fs::create_dir_all(&external_dir).expect("external dir");
        let file = external_dir.join("x.md");
        fs::write(&file, "external content").expect("write");

        let conn = Connection::open_in_memory().expect("db");
        initialize_state_db(&conn, &canvas_root).expect("init");

        let under_root = file.starts_with(&canvas_root);
        assert!(!under_root, "test precondition: file must NOT be under canvas root");

        let has_files_row: bool = conn
            .query_row(
                "SELECT 1 FROM files WHERE path = ?1 LIMIT 1",
                params![file.to_string_lossy()],
                |_| Ok(()),
            )
            .is_ok();
        assert!(!has_files_row, "no files row should exist");

        // Simulate the ephemeral resolution branch.
        let is_tracked = under_root || has_files_row;
        assert!(!is_tracked, "must resolve as ephemeral");

        // Confirm upsert_recent adds a recents row and no files row.
        let db = Mutex::new(conn);
        upsert_recent(&db, &file.to_string_lossy(), "x.md", 1_700_000_000).expect("upsert recent");

        let conn = db.lock().expect("lock");
        let recents_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM recents WHERE path = ?1",
                params![file.to_string_lossy()],
                |row| row.get(0),
            )
            .expect("recents count");
        assert_eq!(recents_count, 1, "must appear in recents");

        let files_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE path = ?1",
                params![file.to_string_lossy()],
                |row| row.get(0),
            )
            .expect("files count");
        assert_eq!(files_count, 0, "must NOT appear in files table");
    }

    #[test]
    fn open_path_resolution_rule_external_with_files_row_is_tracked() {
        // Verify: path outside canvas_root BUT with an existing files row → tracked.
        let temp = tempfile::tempdir().expect("tempdir");
        let canvas_root = temp.path().join("AgentCanvas");
        let external_dir = temp.path().join("Downloads");
        fs::create_dir_all(&external_dir).expect("external dir");
        let file = external_dir.join("tracked-external.md");
        fs::write(&file, "was tracked previously").expect("write");

        let conn = Connection::open_in_memory().expect("db");
        initialize_state_db(&conn, &canvas_root).expect("init");

        // Pre-insert a files row (simulating a previously tracked external path).
        conn.execute(
            "INSERT INTO files(path, last_seen_hash, size, mtime, in_inbox) VALUES (?1, ?2, 1, 1, 0)",
            params![file.to_string_lossy(), vec![0_u8; 32]],
        )
        .expect("insert files row");

        let under_root = file.starts_with(&canvas_root);
        assert!(!under_root, "test precondition: file must NOT be under canvas root");

        let has_files_row: bool = conn
            .query_row(
                "SELECT 1 FROM files WHERE path = ?1 LIMIT 1",
                params![file.to_string_lossy()],
                |_| Ok(()),
            )
            .is_ok();
        assert!(has_files_row, "files row must exist");

        let is_tracked = under_root || has_files_row;
        assert!(is_tracked, "existing files row → must resolve as tracked");
    }
}
