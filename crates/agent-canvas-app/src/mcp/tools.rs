use std::{fs, path::Path};

use base64::{Engine as _, engine::general_purpose};
use rusqlite::{Connection, params, params_from_iter};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;
use vellum_core::sidecar::{self, Comment, CommentAnchor, IdentityMap};

use crate::{
    AgentCanvasPaths, ensure_regular_file, hydrate_file_state, metadata_for_file,
    path_safe_for_canvas, unix_now, upsert_file_state, vault_root_for_absolute_doc,
};

use super::sessions::{self, McpSession};

const TOOL_NAMES: [&str; 9] = [
    "list_artifacts",
    "get_artifact",
    "get_current_focus",
    "get_comments",
    "get_user_messages",
    "open_artifact",
    "notify_user",
    "attach_artifact",
    "add_comment",
];

pub fn tool_schemas() -> Value {
    json!(
        TOOL_NAMES
            .iter()
            .map(|name| tool_schema(name))
            .collect::<Vec<_>>()
    )
}

pub fn call_tool(
    conn: &Connection,
    paths: &AgentCanvasPaths,
    current_focus: Option<String>,
    session: Option<&McpSession>,
    app_handle: Option<&AppHandle>,
    name: &str,
    arguments: Value,
) -> Result<Value, Value> {
    match name {
        "list_artifacts" => list_artifacts(conn, paths, session, arguments),
        "get_artifact" => get_artifact(arguments),
        "get_current_focus" => Ok(tool_result(
            current_focus
                .map(|path| json!({ "path": path }))
                .unwrap_or(Value::Null),
        )),
        "get_comments" => get_comments(arguments),
        "get_user_messages" => get_user_messages(
            conn,
            session.map(|session| session.session_id.as_str()),
            arguments,
        ),
        "open_artifact" => open_artifact(conn, paths, app_handle, arguments),
        // notify_user: DB insert happens here (under the lock); emit happens post-lock
        // via the dispatcher (app_handle is always None when called from handle_tools_call).
        "notify_user" => notify_user(conn, session, arguments),
        "attach_artifact" => attach_artifact(conn, paths, session, app_handle, arguments),
        "add_comment" => add_comment(conn, session, app_handle, arguments),
        _ => Err(json!({
            "code": -32601,
            "message": "unknown tool"
        })),
    }
}

fn list_artifacts(
    conn: &Connection,
    paths: &AgentCanvasPaths,
    session: Option<&McpSession>,
    arguments: Value,
) -> Result<Value, Value> {
    let filter_provided = arguments.get("filter").is_some();
    let filter = arguments
        .get("filter")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let inbox = filter.get("inbox").and_then(Value::as_bool);
    let pinned = filter.get("pinned").and_then(Value::as_bool);
    let archived = filter.get("archived").and_then(Value::as_bool);
    let project = filter.get("project").and_then(Value::as_str);

    let mut clauses = Vec::new();
    let mut values = Vec::new();
    if let Some(inbox) = inbox {
        clauses.push(format!("in_inbox = {}", if inbox { 1 } else { 0 }));
    }
    if let Some(pinned) = pinned {
        clauses.push(format!("pinned = {}", if pinned { 1 } else { 0 }));
    }
    if let Some(archived) = archived {
        clauses.push(format!("archived = {}", if archived { 1 } else { 0 }));
    }
    if let Some(project) = project {
        values.push(project.to_owned());
        clauses.push(format!("project_tag = ?{}", values.len()));
    }

    if !filter_provided {
        let mut default_clauses = vec!["in_inbox = 1".to_owned(), "pinned = 1".to_owned()];
        if let Some(session) = session {
            if !session.project.is_empty() && session.project != "default" {
                values.push(session.project.clone());
                default_clauses.push(format!("project_tag = ?{}", values.len()));
            }
            values.push(session.session_id.clone());
            default_clauses.push(format!(
                "path IN (SELECT path FROM session_attachments WHERE session_id = ?{})",
                values.len()
            ));
        }
        clauses.push(format!("({})", default_clauses.join(" OR ")));
    }

    let where_clause = if clauses.is_empty() {
        "1 = 1".to_owned()
    } else {
        clauses.join(" AND ")
    };
    let sql = format!("SELECT path FROM files WHERE {where_clause}");
    let mut statement = conn
        .prepare(&sql)
        .map_err(|error| rpc_error(-32603, error.to_string()))?;
    let artifact_paths = statement
        .query_map(params_from_iter(values), |row| row.get::<_, String>(0))
        .map_err(|error| rpc_error(-32603, error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| rpc_error(-32603, error.to_string()))?;

    let mut artifacts = Vec::new();
    for path in artifact_paths {
        let path = Path::new(&path);
        if !path.exists() {
            continue;
        }
        let mut file = metadata_for_file(path, &paths.canvas_root)
            .map_err(|error| rpc_error(-32603, error))?;
        hydrate_file_state(conn, &mut file).map_err(|error| rpc_error(-32603, error))?;
        artifacts.push(json!({
            "path": file.path,
            "name": file.name,
            "kind": artifact_kind(path),
            "mtime": file.mtime,
            "persona": file.persona,
            "comment_count": file.comment_count
        }));
    }

    Ok(tool_result(json!(artifacts)))
}

fn open_artifact(
    conn: &Connection,
    paths: &AgentCanvasPaths,
    _app_handle: Option<&AppHandle>,
    arguments: Value,
) -> Result<Value, Value> {
    let path = required_path(&arguments)?;
    ensure_regular_file(&path).map_err(|error| rpc_error(-32602, error))?;
    let path_string = path.to_string_lossy().into_owned();
    let was_already_known = file_is_tracked(conn, &path_string)?;
    if !was_already_known {
        let file = metadata_for_file(&path, &paths.canvas_root)
            .map_err(|error| rpc_error(-32603, error))?;
        upsert_file_state(conn, &file).map_err(|error| rpc_error(-32603, error))?;
        conn.execute(
            "UPDATE files SET in_inbox = 1, archived = 0 WHERE path = ?1",
            params![path_string],
        )
        .map_err(|error| rpc_error(-32603, error.to_string()))?;
    }

    // NOTE: watcher resync, window.show/set_focus/emit, and current_focus update are NOT
    // performed here.  They run in the dispatcher (mcp/mod.rs handle_tools_call) AFTER
    // the db MutexGuard has been dropped, to prevent the reentrant-lock deadlock.

    Ok(tool_result(json!({
        "tracked": true,
        "was_already_known": was_already_known
    })))
}

fn attach_artifact(
    conn: &Connection,
    paths: &AgentCanvasPaths,
    session: Option<&McpSession>,
    _app_handle: Option<&AppHandle>,
    arguments: Value,
) -> Result<Value, Value> {
    let session = session.ok_or_else(|| rpc_error(-32600, "initialize required".to_owned()))?;
    let path = required_path(&arguments)?;
    ensure_regular_file(&path).map_err(|error| rpc_error(-32602, error))?;
    let path_string = path.to_string_lossy().into_owned();
    let also_pin = arguments
        .get("also_pin")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if also_pin || !file_is_tracked(conn, &path_string)? {
        let file = metadata_for_file(&path, &paths.canvas_root)
            .map_err(|error| rpc_error(-32603, error))?;
        upsert_file_state(conn, &file).map_err(|error| rpc_error(-32603, error))?;
    }
    sessions::attach_artifact(conn, &session.session_id, &path_string, unix_now())
        .map_err(|error| rpc_error(-32603, error))?;
    if also_pin {
        conn.execute(
            "UPDATE files SET pinned = 1 WHERE path = ?1",
            params![path_string],
        )
        .map_err(|error| rpc_error(-32603, error.to_string()))?;
    }
    // NOTE: watcher resync is NOT performed here.  It runs in the dispatcher
    // (mcp/mod.rs handle_tools_call) AFTER the db MutexGuard has been dropped,
    // to prevent the reentrant-lock deadlock.
    Ok(tool_result(json!({ "attached": true })))
}

/// notify_user: validate arguments, persist to `agent_messages`, then return.
/// The caller (dispatcher in mod.rs) performs the Tauri window emit AFTER the db
/// guard is released (lock discipline — no window ops while holding state.db).
/// Returns the generated message id so the dispatcher can include it in the post-lock
/// emit payload if desired. On success returns `{ "delivered": true }`.
fn notify_user(conn: &Connection, session: Option<&McpSession>, arguments: Value) -> Result<Value, Value> {
    let severity = arguments
        .get("severity")
        .and_then(Value::as_str)
        .ok_or_else(|| rpc_error(-32602, "severity is required".to_owned()))?;
    if !matches!(severity, "info" | "warn" | "error") {
        return Err(rpc_error(
            -32602,
            "severity must be info, warn, or error".to_owned(),
        ));
    }
    let message = arguments
        .get("message")
        .and_then(Value::as_str)
        .ok_or_else(|| rpc_error(-32602, "message is required".to_owned()))?;
    let action = arguments.get("action").cloned();
    let (action_artifact_path, action_label) = if let Some(action) = action.as_ref() {
        let action_path = action
            .get("artifact_path")
            .and_then(Value::as_str)
            .ok_or_else(|| rpc_error(-32602, "action.artifact_path is required".to_owned()))?;
        let label = action
            .get("label")
            .and_then(Value::as_str)
            .ok_or_else(|| rpc_error(-32602, "action.label is required".to_owned()))?;
        path_safe_for_canvas(Path::new(action_path)).map_err(|error| rpc_error(-32602, error))?;
        (Some(action_path), Some(label))
    } else {
        (None, None)
    };

    // Persist the message to the DB under the current lock.
    let session_id = session.map(|s| s.session_id.as_str()).unwrap_or("unknown");
    let created_at = crate::unix_now();
    let msg_id = sessions::insert_agent_message(
        conn,
        session_id,
        severity,
        message,
        action_artifact_path,
        action_label,
        created_at,
    )
    .map_err(|error| rpc_error(-32603, error))?;

    // Return the id so the post-lock dispatcher can carry the event payload.
    Ok(tool_result(json!({ "delivered": true, "id": msg_id })))
}

fn add_comment(
    conn: &Connection,
    session: Option<&McpSession>,
    app_handle: Option<&AppHandle>,
    arguments: Value,
) -> Result<Value, Value> {
    let session = session.ok_or_else(|| rpc_error(-32600, "initialize required".to_owned()))?;
    let path = required_path(&arguments)?;
    ensure_regular_file(&path).map_err(|error| rpc_error(-32602, error))?;
    let path_string = path.to_string_lossy().into_owned();
    if !file_is_tracked(conn, &path_string)? {
        return Err(rpc_error(-32602, "artifact is not tracked".to_owned()));
    }
    let anchor = arguments
        .get("anchor")
        .cloned()
        .ok_or_else(|| rpc_error(-32602, "anchor is required".to_owned()))
        .and_then(parse_comment_anchor)?;
    let body = arguments
        .get("body")
        .and_then(Value::as_str)
        .ok_or_else(|| rpc_error(-32602, "body is required".to_owned()))?
        .to_owned();
    let bytes = fs::read(&path).map_err(|error| rpc_error(-32603, error.to_string()))?;
    let vault_root =
        vault_root_for_absolute_doc(&path).map_err(|error| rpc_error(-32603, error))?;
    let mut identity = sidecar::load_or_migrate(vault_root, &path, &bytes)
        .map_err(|error| rpc_error(-32603, error.to_string()))?
        .unwrap_or_else(|| IdentityMap {
            source_hash: *blake3::hash(&bytes).as_bytes(),
            block_ids: Vec::new(),
            base_snapshot: None,
            comments: None,
        });
    let comment_id = Uuid::new_v4().to_string();
    let mut comments = identity.comments.unwrap_or_default();
    comments.push(Comment {
        id: comment_id.clone(),
        author: format!("{}·{}", session.persona, session.agent),
        created_at: unix_now(),
        anchor,
        body,
        resolved: false,
    });
    identity.comments = Some(comments);
    sidecar::save(vault_root, &path, &identity)
        .map_err(|error| rpc_error(-32603, error.to_string()))?;
    if let Some(app_handle) = app_handle
        && let Some(window) = app_handle.get_webview_window("main")
    {
        let _ = window.emit(
            "agentcanvas://comments-changed",
            json!({ "path": path_string }),
        );
    }
    Ok(tool_result(json!({ "comment_id": comment_id })))
}

fn get_artifact(arguments: Value) -> Result<Value, Value> {
    let path = required_path(&arguments)?;
    let bytes = fs::read(&path).map_err(|error| rpc_error(-32603, error.to_string()))?;
    let kind = artifact_kind(&path);
    let base_hash = blake3::hash(&bytes).to_hex().to_string();
    let vault_root =
        vault_root_for_absolute_doc(&path).map_err(|error| rpc_error(-32603, error))?;
    let sidecar = sidecar::load_or_migrate(vault_root, &path, &bytes)
        .map_err(|error| rpc_error(-32603, error.to_string()))?;

    let mut result = json!({
        "base_hash": base_hash,
        "sidecar": sidecar,
        "kind": kind
    });
    if text_kind(&kind) {
        result["source"] =
            json!(String::from_utf8(bytes).map_err(|error| rpc_error(-32603, error.to_string()))?);
    } else {
        result["source"] = json!(general_purpose::STANDARD.encode(&bytes));
        result["source_encoding"] = json!("base64");
    }

    Ok(tool_result(result))
}

fn get_comments(arguments: Value) -> Result<Value, Value> {
    let path = required_path(&arguments)?;
    let since = arguments.get("since").and_then(Value::as_i64);
    let bytes = fs::read(&path).map_err(|error| rpc_error(-32603, error.to_string()))?;
    let vault_root =
        vault_root_for_absolute_doc(&path).map_err(|error| rpc_error(-32603, error))?;
    let comments = sidecar::load_or_migrate(vault_root, &path, &bytes)
        .map_err(|error| rpc_error(-32603, error.to_string()))?
        .and_then(|identity| identity.comments)
        .unwrap_or_default()
        .into_iter()
        .filter(|comment| since.is_none_or(|since| comment.created_at >= since))
        .collect::<Vec<_>>();
    Ok(tool_result(json!(comments)))
}

fn get_user_messages(
    conn: &Connection,
    session_id: Option<&str>,
    arguments: Value,
) -> Result<Value, Value> {
    let session_id =
        session_id.ok_or_else(|| rpc_error(-32600, "initialize required".to_owned()))?;
    let since = arguments.get("since").and_then(Value::as_i64);
    let mut sql = "SELECT id, session_id, path, note, action_verb, created_at FROM user_messages WHERE session_id = ?1".to_owned();
    if since.is_some() {
        sql.push_str(" AND created_at >= ?2");
    }
    sql.push_str(" ORDER BY created_at ASC, id ASC");
    let mut statement = conn
        .prepare(&sql)
        .map_err(|error| rpc_error(-32603, error.to_string()))?;

    let mut values = vec![session_id.to_owned()];
    if let Some(since) = since {
        values.push(since.to_string());
    }
    let messages = statement
        .query_map(params_from_iter(values), |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "session_id": row.get::<_, String>(1)?,
                "path": row.get::<_, String>(2)?,
                "note": row.get::<_, Option<String>>(3)?,
                "action_verb": row.get::<_, Option<String>>(4)?,
                "created_at": row.get::<_, i64>(5)?,
            }))
        })
        .map_err(|error| rpc_error(-32603, error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| rpc_error(-32603, error.to_string()))?;
    Ok(tool_result(json!(messages)))
}

fn rpc_error(code: i64, message: String) -> Value {
    json!({ "code": code, "message": message })
}

fn file_is_tracked(conn: &Connection, path: &str) -> Result<bool, Value> {
    conn.query_row("SELECT 1 FROM files WHERE path = ?1", params![path], |_| {
        Ok(())
    })
    .map(|_| true)
    .or_else(|error| {
        if matches!(error, rusqlite::Error::QueryReturnedNoRows) {
            Ok(false)
        } else {
            Err(rpc_error(-32603, error.to_string()))
        }
    })
}

fn parse_comment_anchor(value: Value) -> Result<CommentAnchor, Value> {
    let anchor = serde_json::from_value::<CommentAnchor>(value)
        .map_err(|error| rpc_error(-32602, format!("invalid anchor: {error}")))?;
    match &anchor {
        CommentAnchor::TextSelection(anchor) => {
            if anchor.start_offset > anchor.end_offset {
                return Err(rpc_error(
                    -32602,
                    "anchor start_offset must be <= end_offset".to_owned(),
                ));
            }
        }
        CommentAnchor::HtmlSelection(anchor) => {
            if anchor.start_offset > anchor.end_offset || anchor.snapshot_text.is_empty() {
                return Err(rpc_error(
                    -32602,
                    "html anchor must include ordered offsets and snapshot_text".to_owned(),
                ));
            }
        }
        CommentAnchor::FileLevel(_) => {}
    }
    Ok(anchor)
}

fn required_path(arguments: &Value) -> Result<std::path::PathBuf, Value> {
    let path = arguments
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| rpc_error(-32602, "path is required".to_owned()))?;
    path_safe_for_canvas(Path::new(path)).map_err(|error| rpc_error(-32602, error))
}

fn artifact_kind(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("md" | "markdown") => "md",
        Some("html" | "htm") => "html",
        Some("json") => "json",
        Some("txt") => "txt",
        Some("png") => "png",
        Some("pdf") => "pdf",
        _ => "other",
    }
}

fn text_kind(kind: &str) -> bool {
    matches!(kind, "md" | "html" | "json" | "txt")
}

fn tool_result(value: Value) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string(&value).unwrap_or_else(|_| "null".to_owned())
        }],
        "structuredContent": value
    })
}

fn tool_schema(name: &str) -> Value {
    match name {
        "list_artifacts" => json!({
            "name": "list_artifacts",
            "description": "List tracked artifacts visible to this session. Returns Vec<ArtifactSummary>.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "filter": {
                        "type": "object",
                        "properties": {
                            "inbox": { "type": "boolean" },
                            "project": { "type": "string" },
                            "pinned": { "type": "boolean" },
                            "archived": { "type": "boolean" }
                        },
                        "additionalProperties": false
                    }
                },
                "additionalProperties": false
            }
        }),
        "get_artifact" => json!({
            "name": "get_artifact",
            "description": "Read full artifact source plus base hash, sidecar, and kind.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"],
                "additionalProperties": false
            }
        }),
        "get_current_focus" => json!({
            "name": "get_current_focus",
            "description": "Return the artifact path the user is currently viewing, or null.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        "get_comments" => json!({
            "name": "get_comments",
            "description": "Return comments on an artifact, optionally newer than an epoch-second timestamp.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "since": { "type": "integer" }
                },
                "required": ["path"],
                "additionalProperties": false
            }
        }),
        "get_user_messages" => json!({
            "name": "get_user_messages",
            "description": "Return Send-back messages targeted at this session, optionally since an epoch-second timestamp.",
            "inputSchema": {
                "type": "object",
                "properties": { "since": { "type": "integer" } },
                "additionalProperties": false
            }
        }),
        "open_artifact" => json!({
            "name": "open_artifact",
            "description": "Foreground AgentCanvas, track the artifact if needed, and focus it in the content pane.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"],
                "additionalProperties": false
            }
        }),
        "notify_user" => json!({
            "name": "notify_user",
            "description": "Show a user-facing toast. Action may point at an artifact.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "severity": { "type": "string", "enum": ["info", "warn", "error"] },
                    "message": { "type": "string" },
                    "action": {
                        "type": "object",
                        "properties": {
                            "label": { "type": "string" },
                            "artifact_path": { "type": "string" }
                        },
                        "required": ["label", "artifact_path"],
                        "additionalProperties": false
                    }
                },
                "required": ["severity", "message"],
                "additionalProperties": false
            }
        }),
        "attach_artifact" => json!({
            "name": "attach_artifact",
            "description": "Mark an artifact as in-context for this agent session.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "also_pin": { "type": "boolean", "default": false }
                },
                "required": ["path"],
                "additionalProperties": false
            }
        }),
        "add_comment" => json!({
            "name": "add_comment",
            "description": "Add a comment to an artifact at a text or file-level anchor.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "anchor": {
                        "oneOf": [
                            {
                                "type": "object",
                                "properties": {
                                    "block_id": { "type": "string" },
                                    "start_offset": { "type": "integer" },
                                    "end_offset": { "type": "integer" }
                                },
                                "required": ["start_offset", "end_offset"],
                                "additionalProperties": false
                            },
                            {
                                "type": "object",
                                "properties": { "kind": { "const": "file_level" } },
                                "required": ["kind"],
                                "additionalProperties": false
                            }
                        ]
                    },
                    "body": { "type": "string" }
                },
                "required": ["path", "anchor", "body"],
                "additionalProperties": false
            }
        }),
        _ => unreachable!("unknown static tool name"),
    }
}
