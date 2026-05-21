use std::{fs, path::Path};

use base64::{Engine as _, engine::general_purpose};
use rusqlite::{Connection, params, params_from_iter};
use serde_json::{Value, json};
use vellum_core::sidecar;

use crate::{
    AgentCanvasPaths, hydrate_file_state, metadata_for_file, path_safe_for_canvas,
    vault_root_for_absolute_doc,
};

pub const UNIMPLEMENTED_TOOL_MESSAGE: &str = "tool not yet implemented in skeleton (Slice 4)";

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
    session_id: Option<&str>,
    name: &str,
    arguments: Value,
) -> Result<Value, Value> {
    match name {
        "list_artifacts" => list_artifacts(conn, paths, arguments),
        "get_artifact" => get_artifact(arguments),
        "get_current_focus" => Ok(tool_result(
            current_focus
                .map(|path| json!({ "path": path }))
                .unwrap_or(Value::Null),
        )),
        "get_comments" => get_comments(arguments),
        "get_user_messages" => get_user_messages(conn, session_id, arguments),
        known if TOOL_NAMES.contains(&known) => Err(json!({
            "code": -32601,
            "message": UNIMPLEMENTED_TOOL_MESSAGE
        })),
        _ => Err(json!({
            "code": -32601,
            "message": "unknown tool"
        })),
    }
}

fn list_artifacts(
    conn: &Connection,
    paths: &AgentCanvasPaths,
    arguments: Value,
) -> Result<Value, Value> {
    let filter = arguments
        .get("filter")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let inbox = filter.get("inbox").and_then(Value::as_bool);
    let pinned = filter.get("pinned").and_then(Value::as_bool);
    let archived = filter.get("archived").and_then(Value::as_bool);
    let project = filter.get("project").and_then(Value::as_str);

    let mut clauses = Vec::new();
    if let Some(inbox) = inbox {
        clauses.push(format!("in_inbox = {}", if inbox { 1 } else { 0 }));
    }
    if let Some(pinned) = pinned {
        clauses.push(format!("pinned = {}", if pinned { 1 } else { 0 }));
    }
    if let Some(archived) = archived {
        clauses.push(format!("archived = {}", if archived { 1 } else { 0 }));
    }
    if project.is_some() {
        clauses.push("project_tag = ?1".to_owned());
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
    let artifact_paths = if let Some(project) = project {
        statement
            .query_map(params![project], |row| row.get::<_, String>(0))
            .map_err(|error| rpc_error(-32603, error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
    } else {
        statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| rpc_error(-32603, error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
    }
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
