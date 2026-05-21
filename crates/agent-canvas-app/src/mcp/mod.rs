pub mod sessions;
pub mod tools;

use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    thread,
};

use serde_json::{Value, json};
use tauri::{AppHandle, Manager};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::{UnixListener, UnixStream},
    sync::{mpsc, watch},
};

use crate::{AppState, home_dir, unix_now, valid_persona_names};
use sessions::McpSession;

struct McpControl {
    socket_path: PathBuf,
    shutdown: watch::Sender<bool>,
    clients: Mutex<Vec<mpsc::UnboundedSender<Value>>>,
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
        clients: Mutex::new(Vec::new()),
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
    notify_all(
        control,
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/shutdown",
            "params": {}
        }),
    );
    let _ = control.shutdown.send(true);
    let socket_path = control.socket_path.clone();
    thread::spawn(move || {
        thread::sleep(std::time::Duration::from_millis(150));
        let _ = fs::remove_file(socket_path);
    });
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
    let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
    if let Ok(mut clients) = control.clients.lock() {
        clients.push(tx.clone());
    }

    let mut active_session: Option<McpSession> = None;

    let writer_task = tauri::async_runtime::spawn(async move {
        while let Some(notification) = rx.recv().await {
            if write_json_line(&mut writer, &notification).await.is_err() {
                break;
            }
        }
    });

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if let Some(response) = dispatch_line(&app_handle, &line, &mut active_session)
                    && tx.send(response).is_err()
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

    if let Some(session) = active_session {
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

fn notify_all(control: &McpControl, notification: Value) {
    if let Ok(mut clients) = control.clients.lock() {
        clients.retain(|client| client.send(notification.clone()).is_ok());
    }
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
    line: &str,
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
        Some("initialize") => Some(handle_initialize(app_handle, id, params, active_session)),
        Some("tools/list") => Some(rpc_result(id, json!({ "tools": tools::tool_schemas() }))),
        Some("tools/call") => Some(handle_tools_call(app_handle, id, params)),
        Some("notifications/initialized") => None,
        Some("ping") => Some(rpc_result(id, json!({}))),
        Some(_) => Some(rpc_error(id, -32601, "method not found")),
        None => Some(rpc_error(id, -32600, "invalid request")),
    }
}

fn handle_initialize(
    app_handle: &AppHandle,
    id: Value,
    params: Value,
    active_session: &mut Option<McpSession>,
) -> Value {
    let state = app_handle.state::<AppState>();
    match state.db.lock() {
        Ok(conn) => handle_initialize_with_conn(id, params, &conn, active_session),
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

fn handle_tools_call(app_handle: &AppHandle, id: Value, params: Value) -> Value {
    let state = app_handle.state::<AppState>();
    let paths = match state.paths() {
        Ok(paths) => paths.clone(),
        Err(error) => return rpc_error(id, -32603, &error),
    };
    match state.db.lock() {
        Ok(conn) => handle_tools_call_with_conn(id, params, &conn, &paths),
        Err(_) => rpc_error(id, -32603, "state db lock poisoned"),
    }
}

fn handle_tools_call_with_conn(
    id: Value,
    params: Value,
    conn: &rusqlite::Connection,
    paths: &crate::AgentCanvasPaths,
) -> Value {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let result = tools::call_tool(conn, paths, name, arguments);

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
    use rusqlite::Connection;
    use serde_json::json;

    use super::*;

    fn test_state() -> (Connection, crate::AgentCanvasPaths) {
        let conn = Connection::open_in_memory().expect("db");
        crate::initialize_state_db(
            &conn,
            &crate::legacy_icloud_canvas_root().expect("legacy root"),
        )
        .expect("init db");
        let temp = tempfile::tempdir().expect("tempdir").keep();
        let paths = crate::AgentCanvasPaths {
            canvas_root: temp.join("AgentCanvas"),
            user_symlink: temp.join("AgentCanvas"),
            inbox_dir: temp.join("AgentCanvas/Inbox"),
            projects_dir: temp.join("AgentCanvas/Projects"),
            archive_dir: temp.join("AgentCanvas/Archive"),
            state_db: temp.join("state.db"),
            persona_registry: temp.join("personas"),
        };
        (conn, paths)
    }

    #[test]
    fn initialize_with_valid_clientinfo_returns_serverinfo() {
        let (conn, _) = test_state();
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
        let (conn, _) = test_state();
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
        let (conn, paths) = test_state();
        let response = handle_tools_call_with_conn(
            json!(4),
            json!({"name":"get_artifact","arguments":{"path":"/tmp/x.md"}}),
            &conn,
            &paths,
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
}
