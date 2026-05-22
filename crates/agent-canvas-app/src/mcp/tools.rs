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

const TOOL_NAMES: [&str; 10] = [
    "list_artifacts",
    "get_artifact",
    "get_current_focus",
    "get_comments",
    "get_user_messages",
    "open_artifact",
    "notify_user",
    "attach_artifact",
    "add_comment",
    "dispatch_interaction",
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
        // dispatch_interaction: DB insert happens here (under the lock); window/emit post-lock
        // via dispatch_interaction's gate in handle_tools_call (lock discipline).
        "dispatch_interaction" => dispatch_interaction(conn, session, arguments),
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

    Ok(tool_result(json!({ "artifacts": artifacts })))
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
    Ok(tool_result(json!({ "comments": comments })))
}

/// get_user_messages — v1.1.0 protocol shape.
///
/// Returns `status IN (submitted, draft)` interactions for the caller's session.
/// Each element: `{ interaction_id, ts, payload: { §4 verbatim } }`.
/// On return, sets `read_at` for rows not yet read (INSIDE the db lock — caller must
/// emit `interaction.read` + `messages-changed` POST-lock).
///
/// The `since` parameter filters by `responded_at` epoch (integer, for backward compat
/// with existing MCP clients that pass an epoch integer).
///
/// Returns: `{ messages: [...], read_interaction_ids: [...] }` — the extra field carries
/// which ids need post-lock lifecycle emits. Canvas MUST NOT expose `read_interaction_ids`
/// to MCP clients; it is consumed by `handle_tools_call` and stripped before sending.
pub fn get_user_messages(
    conn: &Connection,
    session_id: Option<&str>,
    arguments: Value,
) -> Result<Value, Value> {
    let session_id =
        session_id.ok_or_else(|| rpc_error(-32600, "initialize required".to_owned()))?;
    let since = arguments.get("since").and_then(Value::as_i64);
    let now_ts = unix_now();

    // Fetch submitted/draft interactions for this session.
    let interactions =
        sessions::get_interactions_submitted_for_session(conn, session_id, since)
            .map_err(|error| rpc_error(-32603, error))?;

    // Mark read_at for rows not yet read, INSIDE the lock.
    // We collect the ids that were newly marked so handle_tools_call can emit post-lock.
    let newly_read =
        sessions::set_interactions_read_at(conn, session_id, now_ts)
            .map_err(|error| rpc_error(-32603, error))?;

    // Build the §5 / v1.1.0 wire elements.
    let messages: Vec<Value> = interactions
        .into_iter()
        .filter_map(|row| {
            // Parse stored response_json to get the §4 payload.
            let payload: Value = row.response_json.as_deref()
                .and_then(|json| serde_json::from_str(json).ok())
                .unwrap_or(Value::Null);

            if !payload.is_object() {
                return None; // skip malformed
            }

            // submitted_at is stored verbatim in response_json.
            let ts = payload
                .get("submitted_at")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();

            // wrapper interaction_id MUST equal payload.interaction_id (spec §5).
            Some(json!({
                "interaction_id": row.interaction_id,
                "ts": ts,
                "payload": payload
            }))
        })
        .collect();

    // Pass read ids back as a side-channel for post-lock emit (stripped by caller before MCP send).
    let read_ids: Vec<Value> = newly_read
        .iter()
        .map(|(id, trace_id, class)| json!({
            "interaction_id": id,
            "trace_id": trace_id,
            "class": class
        }))
        .collect();

    Ok(tool_result(json!({
        "messages": messages,
        "_read_lifecycle": read_ids
    })))
}

/// Returns `(artifact_path, artifact_inline)` or an error.
fn validate_artifact_source(arguments: &Value, class: &str) -> Result<(Option<String>, Option<String>), Value> {
    let artifact_path = arguments.get("artifact_path").and_then(Value::as_str).map(str::to_owned);
    let artifact_inline = arguments.get("artifact_inline").and_then(Value::as_str).map(str::to_owned);
    match (&artifact_path, &artifact_inline) {
        (Some(_), Some(_)) => Err(rpc_error(-32602, "artifact_path and artifact_inline are mutually exclusive".to_owned())),
        (None, Some(_)) if class != "visual-artifact" => Err(rpc_error(-32602, "artifact_inline is only valid for visual-artifact".to_owned())),
        _ => Ok((artifact_path, artifact_inline)),
    }
}

/// dispatch_interaction — agent → Canvas.
///
/// Validates the §3 envelope, inserts a row (status=pending), stores the raw envelope
/// in request_json. Returns `{ dispatched: true, interaction_id }`.
/// Window raise + lifecycle emit happen post-lock in handle_tools_call.
pub fn dispatch_interaction(
    conn: &Connection,
    session: Option<&McpSession>,
    arguments: Value,
) -> Result<Value, Value> {
    let session = session.ok_or_else(|| rpc_error(-32600, "initialize required".to_owned()))?;

    // Required fields.
    let interaction_id = arguments
        .get("interaction_id")
        .and_then(Value::as_str)
        .ok_or_else(|| rpc_error(-32602, "interaction_id is required".to_owned()))?
        .to_owned();

    let class = arguments
        .get("class")
        .and_then(Value::as_str)
        .ok_or_else(|| rpc_error(-32602, "class is required".to_owned()))?;

    // Validate class.
    match class {
        "decision-set" | "document-review" | "approval-gate" | "visual-artifact" => {}
        other => {
            return Err(rpc_error(
                -32602,
                format!("unknown interaction class: {other}; expected one of decision-set, document-review, approval-gate, visual-artifact"),
            ))
        }
    }

    // decision-set requires questions[].
    if class == "decision-set" {
        let questions = arguments.get("questions").and_then(Value::as_array);
        match questions {
            None => return Err(rpc_error(-32602, "decision-set requires questions[]".to_owned())),
            Some(q) if q.is_empty() => return Err(rpc_error(-32602, "decision-set questions[] must not be empty".to_owned())),
            _ => {}
        }
    }

    // Validate artifact source (mutually exclusive; inline only for visual-artifact).
    let (artifact_path, artifact_inline) = validate_artifact_source(&arguments, class)?;

    let title = arguments.get("title").and_then(Value::as_str).map(str::to_owned);
    let trace_id = arguments.get("trace_id").and_then(Value::as_str).map(str::to_owned);
    let now_ts = unix_now();

    sessions::insert_interaction(
        conn,
        &interaction_id,
        &session.session_id,
        class,
        title.as_deref(),
        artifact_path.as_deref(),
        artifact_inline.as_deref(),
        trace_id.as_deref(),
        &serde_json::to_string(&arguments).unwrap_or_default(),
        now_ts,
    )
    .map_err(|error| rpc_error(-32603, error))?;

    Ok(tool_result(json!({
        "dispatched": true,
        "interaction_id": interaction_id,
        // Pass trace_id and class as side-channel for post-lock lifecycle emit.
        // Stripped before MCP send by handle_tools_call.
        "_dispatch_meta": {
            "interaction_id": interaction_id,
            "trace_id": trace_id,
            "class": class
        }
    })))
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
    // MCP requires `structuredContent` to be a JSON object (a record). A bare
    // array fails client-side deserialization ("expected record, received
    // array") and the whole tool call errors — this silently broke
    // get_user_messages / get_comments / list_artifacts for real MCP clients.
    // List-returning tools must wrap their payload in a named object
    // (e.g. { "messages": [...] }). As a backstop, only emit structuredContent
    // when the value is actually an object; otherwise omit it (the `content`
    // text still carries the data) so we never ship an invalid envelope.
    let mut result = json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string(&value).unwrap_or_else(|_| "null".to_owned())
        }]
    });
    if value.is_object() {
        result["structuredContent"] = value;
    } else {
        debug_assert!(
            value.is_object(),
            "tool_result expects an object for structuredContent; wrap arrays in a named key"
        );
    }
    result
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
            "description": "Return structured interaction responses for this session (protocol v1.1.0). Each element: { interaction_id, ts (ISO-8601 Z), payload: { §4 return contract } }. Sets read_at on returned rows. Filter with since (epoch-second integer).",
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
        "dispatch_interaction" => json!({
            "name": "dispatch_interaction",
            "description": "Dispatch a typed interaction (decision-set, document-review, approval-gate, visual-artifact) to the Canvas operator. Canvas renders the class-specific widget and returns a structured response via get_user_messages. Protocol v1.1.0.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "interaction_id": { "type": "string", "description": "Unique ID for this interaction; correlates request <-> response." },
                    "class": {
                        "type": "string",
                        "enum": ["decision-set", "document-review", "approval-gate", "visual-artifact"],
                        "description": "Interaction class."
                    },
                    "title": { "type": "string", "description": "Optional display title." },
                    "artifact_path": { "type": "string", "description": "Absolute path to a .md/.html file (mutually exclusive with artifact_inline)." },
                    "artifact_inline": { "type": "string", "description": "Inline HTML string; visual-artifact only (mutually exclusive with artifact_path)." },
                    "questions": {
                        "type": "array",
                        "description": "Required for decision-set. AskUserQuestion-shaped questions.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "question_id": { "type": "string" },
                                "question": { "type": "string" },
                                "header": { "type": "string" },
                                "multiSelect": { "type": "boolean" },
                                "options": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "key": { "type": "string" },
                                            "label": { "type": "string" },
                                            "description": { "type": "string" },
                                            "recommended": { "type": "boolean" }
                                        },
                                        "required": ["key", "label"]
                                    }
                                }
                            },
                            "required": ["question_id", "question", "options"]
                        }
                    },
                    "fallback": { "type": "string" },
                    "trace_id": { "type": "string", "description": "Links to the handoff-event boundary record; echoed to response." }
                },
                "required": ["interaction_id", "class"],
                "additionalProperties": false
            }
        }),
        _ => unreachable!("unknown static tool name"),
    }
}
