pub mod notifications;
pub mod sessions;
pub mod tools;

use std::{
    fs,
    path::PathBuf,
    sync::{Arc, OnceLock},
    thread,
};

use notifications::JsonRpcNotification;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::{UnixListener, UnixStream},
    sync::{mpsc, watch},
};

use crate::{AppState, home_dir, unix_now, valid_persona_names};
use sessions::{McpSession, SubscriptionRegistry};

struct McpControl {
    socket_path: PathBuf,
    shutdown: watch::Sender<bool>,
    subscriptions: SubscriptionRegistry,
}

static MCP_CONTROL: OnceLock<Arc<McpControl>> = OnceLock::new();

pub fn init_mcp_server(app_handle: AppHandle) -> Result<(), String> {
    let socket_path = mcp_socket_path()?;
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let _ = fs::remove_file(&socket_path);

    let (shutdown, shutdown_rx) = watch::channel(false);
    let control = Arc::new(McpControl {
        socket_path: socket_path.clone(),
        shutdown,
        subscriptions: SubscriptionRegistry::default(),
    });
    let _ = MCP_CONTROL.set(Arc::clone(&control));

    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_listener(app_handle, control, shutdown_rx).await {
            eprintln!("AgentCanvas MCP server stopped: {error}");
        }
    });

    Ok(())
}

pub fn shutdown_mcp_server() {
    let Some(control) = MCP_CONTROL.get() else {
        return;
    };
    notify_all(control, JsonRpcNotification::shutdown());
    let _ = control.shutdown.send(true);
    let socket_path = control.socket_path.clone();
    thread::spawn(move || {
        thread::sleep(std::time::Duration::from_millis(150));
        let _ = fs::remove_file(socket_path);
    });
}

pub fn emit_artifact_updated(
    path: String,
    by: &str,
    note: Option<String>,
    action_verb: Option<String>,
) -> usize {
    let Some(control) = MCP_CONTROL.get() else {
        return 0;
    };
    notifications::dispatch_artifact_updated(&control.subscriptions, path, by, note, action_verb)
}

pub fn emit_artifact_focused(path: String) -> usize {
    let Some(control) = MCP_CONTROL.get() else {
        return 0;
    };
    notifications::dispatch_artifact_focused(&control.subscriptions, path)
}

fn mcp_socket_path() -> Result<PathBuf, String> {
    Ok(home_dir()?
        .join("Library")
        .join("Application Support")
        .join("AgentCanvas")
        .join("mcp.sock"))
}

async fn run_listener(
    app_handle: AppHandle,
    control: Arc<McpControl>,
    shutdown_rx: watch::Receiver<bool>,
) -> Result<(), String> {
    let listener = UnixListener::bind(&control.socket_path).map_err(|error| error.to_string())?;

    loop {
        if *shutdown_rx.borrow() {
            break;
        }
        let (stream, _) = listener.accept().await.map_err(|error| error.to_string())?;
        let connection_control = Arc::clone(&control);
        let connection_app = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            handle_connection(connection_app, connection_control, stream).await;
        });
    }

    Ok(())
}

async fn handle_connection(app_handle: AppHandle, control: Arc<McpControl>, stream: UnixStream) {
    let (read_half, write_half) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();
    let mut writer = BufWriter::new(write_half);
    let (response_tx, mut response_rx) = mpsc::unbounded_channel::<Value>();
    let (notification_tx, mut notification_rx) = mpsc::unbounded_channel::<JsonRpcNotification>();

    let mut active_session: Option<McpSession> = None;

    let writer_task = tauri::async_runtime::spawn(async move {
        while let Some(response) = response_rx.recv().await {
            if write_json_line(&mut writer, &response).await.is_err() {
                break;
            }
        }
    });
    let notification_response_tx = response_tx.clone();
    let notification_bridge_task = tauri::async_runtime::spawn(async move {
        while let Some(notification) = notification_rx.recv().await {
            if notification_response_tx
                .send(notification.to_value())
                .is_err()
            {
                break;
            }
        }
    });

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if let Some(response) = dispatch_line(
                    &app_handle,
                    &control,
                    &line,
                    &notification_tx,
                    &mut active_session,
                ) && response_tx.send(response).is_err()
                {
                    break;
                }
            }
            Ok(None) => break,
            Err(error) => {
                eprintln!("AgentCanvas MCP connection read error: {error}");
                break;
            }
        }
    }

    writer_task.abort();
    notification_bridge_task.abort();

    if let Some(session) = active_session {
        control.subscriptions.remove(&session.session_id);
        let state = app_handle.state::<AppState>();
        if let Ok(conn) = state.db.lock() {
            let _ = sessions::disconnect_agent_session(
                &conn,
                &session.session_id,
                session.connected_at,
                unix_now(),
            );
        }
    }
}

fn notify_all(control: &McpControl, notification: JsonRpcNotification) {
    control.subscriptions.dispatch_all(notification);
}

async fn write_json_line<W>(writer: &mut W, value: &Value) -> Result<(), std::io::Error>
where
    W: AsyncWriteExt + Unpin,
{
    writer.write_all(value.to_string().as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await
}

fn dispatch_line(
    app_handle: &AppHandle,
    control: &McpControl,
    line: &str,
    notification_tx: &mpsc::UnboundedSender<JsonRpcNotification>,
    active_session: &mut Option<McpSession>,
) -> Option<Value> {
    let parsed: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) => return Some(rpc_error(Value::Null, -32700, "parse error")),
    };
    let id = parsed.get("id").cloned().unwrap_or(Value::Null);
    let method = parsed.get("method").and_then(Value::as_str);
    let params = parsed.get("params").cloned().unwrap_or_else(|| json!({}));

    match method {
        Some("initialize") => Some(handle_initialize(
            app_handle,
            control,
            notification_tx.clone(),
            id,
            params,
            active_session,
        )),
        Some("tools/list") => Some(rpc_result(id, json!({ "tools": tools::tool_schemas() }))),
        Some("tools/call") => Some(handle_tools_call(
            app_handle,
            id,
            params,
            active_session.as_ref(),
        )),
        Some("notifications/subscribe") => Some(handle_subscribe(
            control,
            id,
            params,
            active_session.as_ref(),
        )),
        Some("notifications/initialized") => None,
        Some("ping") => Some(rpc_result(id, json!({}))),
        Some(_) => Some(rpc_error(id, -32601, "method not found")),
        None => Some(rpc_error(id, -32600, "invalid request")),
    }
}

fn handle_initialize(
    app_handle: &AppHandle,
    control: &McpControl,
    notification_tx: mpsc::UnboundedSender<JsonRpcNotification>,
    id: Value,
    params: Value,
    active_session: &mut Option<McpSession>,
) -> Value {
    let state = app_handle.state::<AppState>();
    match state.db.lock() {
        Ok(conn) => {
            let response = handle_initialize_with_conn(id, params, &conn, active_session);
            if response.get("result").is_some()
                && let Some(session) = active_session.as_ref()
            {
                control
                    .subscriptions
                    .register_default(session.session_id.clone(), notification_tx);
            }
            response
        }
        Err(_) => rpc_error(id, -32603, "state db lock poisoned"),
    }
}

fn handle_initialize_with_conn(
    id: Value,
    params: Value,
    conn: &rusqlite::Connection,
    active_session: &mut Option<McpSession>,
) -> Value {
    let protocol_version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or("2025-06-18");
    let agent_canvas = params
        .get("clientInfo")
        .and_then(|client_info| client_info.get("agentCanvas"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let persona = agent_canvas
        .get("persona")
        .and_then(Value::as_str)
        .unwrap_or("default");
    if !valid_persona_names().contains(persona) {
        eprintln!("AgentCanvas MCP initialize used unknown persona: {persona}");
    }
    let agent = agent_canvas
        .get("agent")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let project = agent_canvas
        .get("project")
        .and_then(Value::as_str)
        .unwrap_or("default");
    let session_id = agent_canvas
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or("unknown-session");
    let connected_at = unix_now();

    let session = match sessions::insert_agent_session(
        conn,
        session_id,
        persona,
        agent,
        project,
        connected_at,
    ) {
        Ok(session) => session,
        Err(error) => return rpc_error(id, -32603, &error),
    };
    *active_session = Some(session);

    rpc_result(
        id,
        json!({
            "protocolVersion": protocol_version,
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "AgentCanvas",
                "version": "0.3.0"
            }
        }),
    )
}

fn handle_tools_call(
    app_handle: &AppHandle,
    id: Value,
    params: Value,
    active_session: Option<&McpSession>,
) -> Value {
    let state = app_handle.state::<AppState>();
    let paths = match state.paths() {
        Ok(paths) => paths.clone(),
        Err(error) => return rpc_error(id, -32603, &error),
    };
    let current_focus = match state.current_focus.lock() {
        Ok(current_focus) => current_focus.clone(),
        Err(_) => return rpc_error(id, -32603, "current focus lock poisoned"),
    };
    match state.db.lock() {
        Ok(conn) => handle_tools_call_with_conn(
            id,
            params,
            &conn,
            &paths,
            current_focus,
            active_session.map(|session| session.session_id.as_str()),
        ),
        Err(_) => rpc_error(id, -32603, "state db lock poisoned"),
    }
}

fn handle_tools_call_with_conn(
    id: Value,
    params: Value,
    conn: &rusqlite::Connection,
    paths: &crate::AgentCanvasPaths,
    current_focus: Option<String>,
    session_id: Option<&str>,
) -> Value {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let result = tools::call_tool(conn, paths, current_focus, session_id, name, arguments);

    match result {
        Ok(value) => rpc_result(id, value),
        Err(error) => {
            let code = error.get("code").and_then(Value::as_i64).unwrap_or(-32603);
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("tool call failed");
            rpc_error(id, code, message)
        }
    }
}

fn handle_subscribe(
    control: &McpControl,
    id: Value,
    params: Value,
    active_session: Option<&McpSession>,
) -> Value {
    let Some(session) = active_session else {
        return rpc_error(id, -32600, "initialize required");
    };
    let request = notifications::parse_subscribe_request(&params);
    control.subscriptions.subscribe(
        &session.session_id,
        request.artifact_updated,
        request.artifact_focused,
    );
    rpc_result(id, json!({}))
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rusqlite::Connection;
    use serde_json::json;
    use tokio::sync::mpsc;
    use vellum_core::sidecar::{
        Comment, CommentAnchor, FileLevelAnchor, FileLevelKind, IdentityMap,
    };

    use super::*;

    fn test_state() -> (Connection, crate::AgentCanvasPaths, tempfile::TempDir) {
        let conn = Connection::open_in_memory().expect("db");
        crate::initialize_state_db(
            &conn,
            &crate::legacy_icloud_canvas_root().expect("legacy root"),
        )
        .expect("init db");
        let temp = tempfile::tempdir_in(std::env::current_dir().expect("cwd")).expect("tempdir");
        let paths = crate::AgentCanvasPaths {
            canvas_root: temp.path().join("AgentCanvas"),
            user_symlink: temp.path().join("AgentCanvas"),
            inbox_dir: temp.path().join("AgentCanvas/Inbox"),
            projects_dir: temp.path().join("AgentCanvas/Projects"),
            archive_dir: temp.path().join("AgentCanvas/Archive"),
            state_db: temp.path().join("state.db"),
            persona_registry: temp.path().join("personas"),
        };
        (conn, paths, temp)
    }

    #[test]
    fn initialize_with_valid_clientinfo_returns_serverinfo() {
        let (conn, _, _temp) = test_state();
        let mut active = None;
        let response = handle_initialize_with_conn(
            json!(1),
            json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"manual-test","version":"0.0.1","agentCanvas":{"persona":"cpo","agent":"claude","project":"agent-canvas","session_id":"manual-test-1"}}}),
            &conn,
            &mut active,
        );

        assert_eq!(response["result"]["serverInfo"]["name"], "AgentCanvas");
        assert_eq!(response["result"]["capabilities"]["tools"], json!({}));
        assert!(active.is_some());
    }

    #[test]
    fn initialize_with_unknown_persona_accepts_with_warning() {
        let (conn, _, _temp) = test_state();
        let mut active = None;
        let response = handle_initialize_with_conn(
            json!(2),
            json!({"clientInfo":{"agentCanvas":{"persona":"unknown-persona","agent":"codex","project":"agent-canvas","session_id":"unknown-1"}}}),
            &conn,
            &mut active,
        );

        assert_eq!(response["result"]["serverInfo"]["name"], "AgentCanvas");
        let persona: String = conn
            .query_row(
                "SELECT persona FROM agent_sessions WHERE session_id = 'unknown-1'",
                [],
                |row| row.get(0),
            )
            .expect("session");
        assert_eq!(persona, "unknown-persona");
    }

    #[test]
    fn tools_list_returns_nine_tools_with_input_schemas() {
        let response = rpc_result(json!(3), json!({ "tools": tools::tool_schemas() }));

        let tools = response["result"]["tools"].as_array().expect("tools");
        assert_eq!(tools.len(), 9);
        assert!(tools.iter().all(|tool| tool.get("inputSchema").is_some()));
        assert!(tools.iter().any(|tool| tool["name"] == "add_comment"));
    }

    #[test]
    fn tools_call_stub_returns_method_not_found_for_unimplemented() {
        let (conn, paths, _temp) = test_state();
        let response = handle_tools_call_with_conn(
            json!(4),
            json!({"name":"open_artifact","arguments":{"path":"/tmp/x.md"}}),
            &conn,
            &paths,
            None,
            Some("s1"),
        );

        assert_eq!(response["error"]["code"], -32601);
        assert_eq!(
            response["error"]["message"],
            tools::UNIMPLEMENTED_TOOL_MESSAGE
        );
    }

    #[test]
    fn agent_sessions_migration_idempotent() {
        let conn = Connection::open_in_memory().expect("db");
        sessions::migrate_agent_sessions(&conn).expect("migration 1");
        sessions::migrate_agent_sessions(&conn).expect("migration 2");

        conn.execute(
            "INSERT INTO agent_sessions(session_id, source, persona, agent, project, connected_at) VALUES ('s1', 'mcp', 'cpo', 'claude', 'agent-canvas', 1)",
            [],
        )
        .expect("insert");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_sessions", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn subscribe_updates_session_mask() {
        let registry = SubscriptionRegistry::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        registry.register_default("s1".to_owned(), tx);

        registry.subscribe("s1", true, true);

        let subscription = registry.get("s1").expect("subscription");
        assert!(subscription.artifact_updated);
        assert!(subscription.artifact_focused);
    }

    #[test]
    fn default_subscription_includes_artifact_updated() {
        let registry = SubscriptionRegistry::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        registry.register_default("s1".to_owned(), tx);

        assert!(registry.get("s1").expect("subscription").artifact_updated);
    }

    #[test]
    fn default_subscription_excludes_artifact_focused() {
        let registry = SubscriptionRegistry::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        registry.register_default("s1".to_owned(), tx);

        assert!(!registry.get("s1").expect("subscription").artifact_focused);
    }

    #[test]
    fn event_dispatch_filters_by_subscription_mask() {
        let registry = SubscriptionRegistry::default();
        let (tx1, mut rx1) = mpsc::unbounded_channel();
        let (tx2, mut rx2) = mpsc::unbounded_channel();
        registry.register_default("s1".to_owned(), tx1);
        registry.register_default("s2".to_owned(), tx2);
        registry.subscribe("s1", false, true);

        let sent = notifications::dispatch_artifact_focused(&registry, "/x.md".to_owned());

        assert_eq!(sent, 1);
        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_err());
    }

    #[test]
    fn get_artifact_returns_base64_for_png() {
        let (conn, paths, _temp) = test_state();
        fs::create_dir_all(&paths.canvas_root).expect("canvas");
        let png = paths.canvas_root.join("tiny.png");
        let bytes = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
        fs::write(&png, bytes).expect("png");

        let response = handle_tools_call_with_conn(
            json!(10),
            json!({"name":"get_artifact","arguments":{"path":png.to_string_lossy()}}),
            &conn,
            &paths,
            None,
            Some("s1"),
        );

        let artifact = &response["result"]["structuredContent"];
        assert_eq!(artifact["kind"], "png");
        assert_eq!(artifact["source_encoding"], "base64");
        assert_eq!(artifact["source"], "iVBORw0KGgo=");
    }

    #[test]
    fn get_artifact_returns_string_for_markdown() {
        let (conn, paths, _temp) = test_state();
        fs::create_dir_all(&paths.canvas_root).expect("canvas");
        let markdown = paths.canvas_root.join("note.md");
        fs::write(&markdown, "# Hi\n").expect("markdown");

        let response = handle_tools_call_with_conn(
            json!(11),
            json!({"name":"get_artifact","arguments":{"path":markdown.to_string_lossy()}}),
            &conn,
            &paths,
            None,
            Some("s1"),
        );

        let artifact = &response["result"]["structuredContent"];
        assert_eq!(artifact["kind"], "md");
        assert_eq!(artifact["source"], "# Hi\n");
        assert!(artifact.get("source_encoding").is_none());
    }

    #[test]
    fn get_comments_respects_since_filter() {
        let (conn, paths, _temp) = test_state();
        fs::create_dir_all(&paths.canvas_root).expect("canvas");
        let markdown = paths.canvas_root.join("comments.md");
        fs::write(&markdown, "# Hi\n").expect("markdown");
        let identity = IdentityMap {
            source_hash: *blake3::hash(b"# Hi\n").as_bytes(),
            block_ids: Vec::new(),
            base_snapshot: None,
            comments: Some(vec![test_comment("old", 10), test_comment("new", 20)]),
        };
        vellum_core::sidecar::save(paths.canvas_root.as_path(), &markdown, &identity)
            .expect("save sidecar");

        let response = handle_tools_call_with_conn(
            json!(12),
            json!({"name":"get_comments","arguments":{"path":markdown.to_string_lossy(),"since":15}}),
            &conn,
            &paths,
            None,
            Some("s1"),
        );

        let comments = response["result"]["structuredContent"]
            .as_array()
            .expect("comments");
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0]["id"], "new");
    }

    #[test]
    fn get_user_messages_filters_by_session_id() {
        let (conn, paths, _temp) = test_state();
        conn.execute(
            "INSERT INTO user_messages(id, session_id, path, note, action_verb, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params!["m1", "s1", "/x.md", "note", "Review", 10],
        )
        .expect("insert m1");
        conn.execute(
            "INSERT INTO user_messages(id, session_id, path, note, action_verb, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params!["m2", "s2", "/x.md", "other", "Review", 20],
        )
        .expect("insert m2");

        let response = handle_tools_call_with_conn(
            json!(13),
            json!({"name":"get_user_messages","arguments":{}}),
            &conn,
            &paths,
            None,
            Some("s1"),
        );

        let messages = response["result"]["structuredContent"]
            .as_array()
            .expect("messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["id"], "m1");
    }

    #[test]
    fn set_current_focus_then_get_current_focus_round_trips() {
        let (conn, paths, _temp) = test_state();
        let response = handle_tools_call_with_conn(
            json!(14),
            json!({"name":"get_current_focus","arguments":{}}),
            &conn,
            &paths,
            Some("/abs/path.md".to_owned()),
            Some("s1"),
        );

        assert_eq!(
            response["result"]["structuredContent"],
            json!({ "path": "/abs/path.md" })
        );
    }

    #[test]
    fn user_messages_migration_idempotent() {
        let conn = Connection::open_in_memory().expect("db");
        sessions::migrate_user_messages(&conn).expect("migration 1");
        sessions::migrate_user_messages(&conn).expect("migration 2");

        conn.execute(
            "INSERT INTO user_messages(id, session_id, path, created_at) VALUES ('m1', 's1', '/x.md', 1)",
            [],
        )
        .expect("insert");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM user_messages", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 1);
    }

    fn test_comment(id: &str, created_at: i64) -> Comment {
        Comment {
            id: id.to_owned(),
            author: "codex".to_owned(),
            created_at,
            anchor: CommentAnchor::FileLevel(FileLevelAnchor {
                kind: FileLevelKind::FileLevel,
            }),
            body: id.to_owned(),
            resolved: false,
        }
    }
}
