use std::path::PathBuf;

use rusqlite::{Connection, params};
use serde_json::{Value, json};

use crate::{AgentCanvasPaths, hydrate_file_state, is_supported_artifact, metadata_for_file};

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
    name: &str,
    arguments: Value,
) -> Result<Value, Value> {
    match name {
        "list_artifacts" => list_artifacts(conn, paths, arguments),
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
    } else {
        clauses.push("archived = 0".to_owned());
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
        let path = PathBuf::from(path);
        if !path.exists() || !is_supported_artifact(&path) {
            continue;
        }
        let mut file = metadata_for_file(&path, &paths.canvas_root)
            .map_err(|error| rpc_error(-32603, error))?;
        hydrate_file_state(conn, &mut file).map_err(|error| rpc_error(-32603, error))?;
        artifacts.push(json!({
            "path": file.path,
            "name": file.name,
            "kind": file.extension,
            "size": file.size,
            "mtime": file.mtime,
            "pinned": file.pinned,
            "archived": file.archived,
            "persona": file.persona,
            "review_state": file.review_state,
            "comment_count": file.comment_count
        }));
    }

    Ok(json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string(&artifacts).unwrap_or_else(|_| "[]".to_owned())
        }],
        "structuredContent": artifacts
    }))
}

fn rpc_error(code: i64, message: String) -> Value {
    json!({ "code": code, "message": message })
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
