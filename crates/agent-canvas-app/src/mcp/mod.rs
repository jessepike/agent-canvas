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

pub fn emit_artifact_updated_to_session(
    session_id: &str,
    path: String,
    by: &str,
    note: Option<String>,
    action_verb: Option<String>,
) -> bool {
    let Some(control) = MCP_CONTROL.get() else {
        return false;
    };
    control.subscriptions.dispatch_to_session(
        session_id,
        JsonRpcNotification::artifact_updated(path, by, note, action_verb),
    )
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
            let _ = sessions::cleanup_session_attachments(&conn, &session.session_id);
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
            active_session,
            Some(app_handle),
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
    session: Option<&McpSession>,
    app_handle: Option<&AppHandle>,
) -> Value {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let result = tools::call_tool(
        conn,
        paths,
        current_focus,
        session,
        app_handle,
        name,
        arguments,
    );

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
    use std::{
        fs,
        time::{Duration, Instant},
    };

    use rusqlite::Connection;
    use serde_json::json;
    use tokio::sync::mpsc;
    use vellum_core::sidecar::{
        Comment, CommentAnchor, FileLevelAnchor, FileLevelKind, IdentityMap,
    };
    use vellum_core::watch;

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

    fn test_session(session_id: &str) -> McpSession {
        McpSession {
            session_id: session_id.to_owned(),
            persona: "cpo".to_owned(),
            agent: "claude".to_owned(),
            project: "agent-canvas".to_owned(),
            connected_at: 1,
        }
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
    fn tools_call_unknown_returns_method_not_found() {
        let (conn, paths, _temp) = test_state();
        let response = handle_tools_call_with_conn(
            json!(4),
            json!({"name":"unknown_tool","arguments":{}}),
            &conn,
            &paths,
            None,
            Some(&test_session("s1")),
            None,
        );

        assert_eq!(response["error"]["code"], -32601);
        assert_eq!(response["error"]["message"], "unknown tool");
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
    fn watcher_change_dispatches_artifact_updated_notification() {
        let temp = tempfile::tempdir_in(std::env::current_dir().expect("cwd")).expect("tempdir");
        let target = temp.path().join("watched.md");
        fs::write(&target, "old").expect("write old");

        let registry = SubscriptionRegistry::default();
        let (tx, mut rx) = mpsc::unbounded_channel();
        registry.register_default("s1".to_owned(), tx);
        registry.subscribe("s1", true, false);
        let watcher_registry = registry.clone();
        let watch = watch::start(move |event| {
            if let watch::WatchEvent::Changed { path, .. } = event {
                notifications::dispatch_artifact_updated(
                    &watcher_registry,
                    path.to_string_lossy().into_owned(),
                    "watcher",
                    None,
                    None,
                );
            }
        })
        .expect("watch start");
        watch.watch_recursive(temp.path()).expect("watch recursive");

        std::thread::sleep(Duration::from_millis(250));
        fs::write(&target, "new").expect("write new");

        let deadline = Instant::now() + Duration::from_millis(1500);
        let notification = loop {
            if let Ok(notification) = rx.try_recv() {
                break notification;
            }
            assert!(
                Instant::now() < deadline,
                "expected artifact_updated notification"
            );
            std::thread::sleep(Duration::from_millis(25));
        };
        assert_eq!(notification.method, "notifications/artifact_updated");
        assert_eq!(notification.params["by"], "watcher");
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
            Some(&test_session("s1")),
            None,
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
            Some(&test_session("s1")),
            None,
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
            Some(&test_session("s1")),
            None,
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
            Some(&test_session("s1")),
            None,
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
            Some(&test_session("s1")),
            None,
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

    #[test]
    fn open_artifact_inserts_unknown_path_with_inbox_tag() {
        let (conn, paths, _temp) = test_state();
        fs::create_dir_all(&paths.canvas_root).expect("canvas");
        let markdown = paths.canvas_root.join("new.md");
        fs::write(&markdown, "# New\n").expect("markdown");

        let response = handle_tools_call_with_conn(
            json!(20),
            json!({"name":"open_artifact","arguments":{"path":markdown.to_string_lossy()}}),
            &conn,
            &paths,
            None,
            Some(&test_session("s1")),
            None,
        );

        assert_eq!(response["result"]["structuredContent"]["tracked"], true);
        assert_eq!(
            response["result"]["structuredContent"]["was_already_known"],
            false
        );
        let in_inbox: i64 = conn
            .query_row(
                "SELECT in_inbox FROM files WHERE path = ?1",
                rusqlite::params![markdown.to_string_lossy()],
                |row| row.get(0),
            )
            .expect("file row");
        assert_eq!(in_inbox, 1);
    }

    #[test]
    fn open_artifact_returns_was_already_known_for_tracked_path() {
        let (conn, paths, _temp) = test_state();
        fs::create_dir_all(&paths.canvas_root).expect("canvas");
        let markdown = paths.canvas_root.join("known.md");
        fs::write(&markdown, "# Known\n").expect("markdown");
        insert_test_file(&conn, &paths, &markdown, 1, None, 0);

        let response = handle_tools_call_with_conn(
            json!(21),
            json!({"name":"open_artifact","arguments":{"path":markdown.to_string_lossy()}}),
            &conn,
            &paths,
            None,
            Some(&test_session("s1")),
            None,
        );

        assert_eq!(
            response["result"]["structuredContent"]["was_already_known"],
            true
        );
    }

    #[test]
    fn attach_artifact_inserts_session_attachment_row() {
        let (conn, paths, _temp) = test_state();
        fs::create_dir_all(&paths.canvas_root).expect("canvas");
        let markdown = paths.canvas_root.join("attach.md");
        fs::write(&markdown, "# Attach\n").expect("markdown");
        let session = test_session("s1");

        let response = handle_tools_call_with_conn(
            json!(22),
            json!({"name":"attach_artifact","arguments":{"path":markdown.to_string_lossy()}}),
            &conn,
            &paths,
            None,
            Some(&session),
            None,
        );

        assert_eq!(response["result"]["structuredContent"]["attached"], true);
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM session_attachments WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn attach_artifact_with_also_pin_pins_file() {
        let (conn, paths, _temp) = test_state();
        fs::create_dir_all(&paths.canvas_root).expect("canvas");
        let markdown = paths.canvas_root.join("pin.md");
        fs::write(&markdown, "# Pin\n").expect("markdown");

        let response = handle_tools_call_with_conn(
            json!(23),
            json!({"name":"attach_artifact","arguments":{"path":markdown.to_string_lossy(),"also_pin":true}}),
            &conn,
            &paths,
            None,
            Some(&test_session("s1")),
            None,
        );

        assert_eq!(response["result"]["structuredContent"]["attached"], true);
        let pinned: i64 = conn
            .query_row(
                "SELECT pinned FROM files WHERE path = ?1",
                rusqlite::params![markdown.to_string_lossy()],
                |row| row.get(0),
            )
            .expect("pinned");
        assert_eq!(pinned, 1);
    }

    #[test]
    fn attach_artifact_cleanup_on_connection_close_removes_rows() {
        let (conn, paths, _temp) = test_state();
        fs::create_dir_all(&paths.canvas_root).expect("canvas");
        let markdown = paths.canvas_root.join("cleanup.md");
        fs::write(&markdown, "# Cleanup\n").expect("markdown");
        sessions::attach_artifact(&conn, "s1", &markdown.to_string_lossy(), 1).expect("attach");

        sessions::cleanup_session_attachments(&conn, "s1").expect("cleanup");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM session_attachments", [], |row| {
                row.get(0)
            })
            .expect("count");
        assert_eq!(count, 0);
    }

    #[test]
    fn add_comment_appends_with_persona_agent_author() {
        let (conn, paths, _temp) = test_state();
        fs::create_dir_all(&paths.canvas_root).expect("canvas");
        let markdown = paths.canvas_root.join("comment.md");
        fs::write(&markdown, "# Comment\n").expect("markdown");
        insert_test_file(&conn, &paths, &markdown, 1, None, 0);

        let response = handle_tools_call_with_conn(
            json!(24),
            json!({"name":"add_comment","arguments":{"path":markdown.to_string_lossy(),"anchor":{"kind":"file_level"},"body":"Looks good"}}),
            &conn,
            &paths,
            None,
            Some(&test_session("s1")),
            None,
        );

        assert!(response["result"]["structuredContent"]["comment_id"].is_string());
        let sidecar = load_identity_for_test(&paths, &markdown);
        assert_eq!(sidecar.comments.expect("comments")[0].author, "cpo·claude");
    }

    #[test]
    fn add_comment_round_trips_through_sidecar() {
        let (conn, paths, _temp) = test_state();
        fs::create_dir_all(&paths.canvas_root).expect("canvas");
        let markdown = paths.canvas_root.join("roundtrip.md");
        fs::write(&markdown, "# Roundtrip\n").expect("markdown");
        insert_test_file(&conn, &paths, &markdown, 1, None, 0);

        handle_tools_call_with_conn(
            json!(25),
            json!({"name":"add_comment","arguments":{"path":markdown.to_string_lossy(),"anchor":{"start_offset":0,"end_offset":3},"body":"Body"}}),
            &conn,
            &paths,
            None,
            Some(&test_session("s1")),
            None,
        );

        let response = handle_tools_call_with_conn(
            json!(26),
            json!({"name":"get_comments","arguments":{"path":markdown.to_string_lossy()}}),
            &conn,
            &paths,
            None,
            Some(&test_session("s1")),
            None,
        );
        let comments = response["result"]["structuredContent"]
            .as_array()
            .expect("comments");
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0]["body"], "Body");
    }

    #[test]
    fn notify_user_emits_tauri_event() {
        let (conn, paths, _temp) = test_state();
        fs::create_dir_all(&paths.canvas_root).expect("canvas");
        let action_path = paths.canvas_root.join("x.md");
        fs::write(&action_path, "# X\n").expect("action file");
        let response = handle_tools_call_with_conn(
            json!(27),
            json!({"name":"notify_user","arguments":{"severity":"warn","message":"Check this","action":{"label":"Open","artifact_path":action_path.to_string_lossy()}}}),
            &conn,
            &paths,
            None,
            Some(&test_session("s1")),
            None,
        );

        assert_eq!(
            response["result"]["structuredContent"],
            json!({"delivered": true})
        );
    }

    #[test]
    fn list_artifacts_default_returns_inbox_plus_project_plus_attached_plus_pinned() {
        let (conn, paths, _temp) = test_state();
        fs::create_dir_all(&paths.canvas_root).expect("canvas");
        let inbox = paths.canvas_root.join("inbox.md");
        let project = paths.canvas_root.join("project.md");
        let attached = paths.canvas_root.join("attached.md");
        let pinned = paths.canvas_root.join("pinned.md");
        for path in [&inbox, &project, &attached, &pinned] {
            fs::write(path, "# File\n").expect("write");
        }
        insert_test_file(&conn, &paths, &inbox, 1, None, 0);
        insert_test_file(&conn, &paths, &project, 0, Some("agent-canvas"), 0);
        insert_test_file(&conn, &paths, &attached, 0, None, 0);
        insert_test_file(&conn, &paths, &pinned, 0, None, 1);
        sessions::attach_artifact(&conn, "s1", &attached.to_string_lossy(), 1).expect("attach");

        let response = handle_tools_call_with_conn(
            json!(28),
            json!({"name":"list_artifacts","arguments":{}}),
            &conn,
            &paths,
            None,
            Some(&test_session("s1")),
            None,
        );
        let paths = response["result"]["structuredContent"]
            .as_array()
            .expect("artifacts")
            .iter()
            .map(|artifact| artifact["path"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();

        assert!(paths.contains(&inbox.to_string_lossy().into_owned()));
        assert!(paths.contains(&project.to_string_lossy().into_owned()));
        assert!(paths.contains(&attached.to_string_lossy().into_owned()));
        assert!(paths.contains(&pinned.to_string_lossy().into_owned()));
    }

    #[test]
    fn send_back_to_session_inserts_user_message_and_emits_notification() {
        let (conn, paths, _temp) = test_state();
        let path = paths.canvas_root.join("send.md");
        sessions::insert_agent_session(&conn, "s1", "cpo", "claude", "agent-canvas", 1)
            .expect("session");
        sessions::attach_artifact(&conn, "s1", &path.to_string_lossy(), 2).expect("attach");
        sessions::insert_user_message(
            &conn,
            "s1",
            &path.to_string_lossy(),
            Some("note"),
            Some("Review"),
            3,
        )
        .expect("message");
        let registry = SubscriptionRegistry::default();
        let (tx, mut rx) = mpsc::unbounded_channel();
        registry.register_default("s1".to_owned(), tx);

        let delivered = registry.dispatch_to_session(
            "s1",
            JsonRpcNotification::artifact_updated(
                path.to_string_lossy().into_owned(),
                "user",
                Some("note".to_owned()),
                Some("Review".to_owned()),
            ),
        );

        assert!(delivered);
        assert_eq!(
            rx.try_recv().expect("notification").method,
            "notifications/artifact_updated"
        );
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM user_messages WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn session_attachments_migration_idempotent() {
        let conn = Connection::open_in_memory().expect("db");
        sessions::migrate_session_attachments(&conn).expect("migration 1");
        sessions::migrate_session_attachments(&conn).expect("migration 2");
        sessions::attach_artifact(&conn, "s1", "/tmp/x.md", 1).expect("attach");
        sessions::attach_artifact(&conn, "s1", "/tmp/x.md", 2).expect("attach again");
        let attached_at: i64 = conn
            .query_row(
                "SELECT attached_at FROM session_attachments WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .expect("attached_at");
        assert_eq!(attached_at, 2);
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

    fn insert_test_file(
        conn: &Connection,
        paths: &crate::AgentCanvasPaths,
        path: &std::path::Path,
        in_inbox: i64,
        project: Option<&str>,
        pinned: i64,
    ) {
        let mut file = crate::metadata_for_file(path, &paths.canvas_root).expect("metadata");
        crate::upsert_file_state(conn, &file).expect("upsert");
        conn.execute(
            "UPDATE files SET in_inbox = ?2, project_tag = ?3, pinned = ?4 WHERE path = ?1",
            rusqlite::params![file.path, in_inbox, project, pinned],
        )
        .expect("update");
        crate::hydrate_file_state(conn, &mut file).expect("hydrate");
    }

    fn load_identity_for_test(
        paths: &crate::AgentCanvasPaths,
        path: &std::path::Path,
    ) -> IdentityMap {
        let bytes = fs::read(path).expect("read");
        vellum_core::sidecar::load_or_migrate(paths.canvas_root.as_path(), path, &bytes)
            .expect("load")
            .expect("identity")
    }
}
