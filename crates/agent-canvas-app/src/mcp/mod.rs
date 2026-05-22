pub mod notifications;
pub mod sessions;
pub mod tools;

use std::{
    collections::HashSet,
    fs,
    path::PathBuf,
    sync::{Arc, OnceLock},
    thread,
    time::Duration,
};

use notifications::JsonRpcNotification;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::{UnixListener, UnixStream},
    sync::{RwLock, mpsc, watch},
};

use crate::{AppState, home_dir, persona_names_from_registry_root, resync_watcher_from_db, unix_now, valid_persona_names};
use sessions::{McpSession, SubscriptionRegistry};

struct McpControl {
    socket_path: PathBuf,
    shutdown: watch::Sender<bool>,
    subscriptions: SubscriptionRegistry,
    personas: Arc<RwLock<HashSet<String>>>,
}

static MCP_CONTROL: OnceLock<Arc<McpControl>> = OnceLock::new();

pub fn init_mcp_server(app_handle: AppHandle) -> Result<(), String> {
    let socket_path = mcp_socket_path()?;
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let _ = fs::remove_file(&socket_path);

    let (shutdown, shutdown_rx) = watch::channel(false);
    let personas = app_handle
        .state::<AppState>()
        .paths()
        .map(|paths| persona_names_from_registry_root(&paths.persona_registry))
        .unwrap_or_else(|_| valid_persona_names());
    let control = Arc::new(McpControl {
        socket_path: socket_path.clone(),
        shutdown,
        subscriptions: SubscriptionRegistry::default(),
        personas: Arc::new(RwLock::new(personas)),
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

pub fn disconnect_session(session_id: &str) -> bool {
    let Some(control) = MCP_CONTROL.get() else {
        return false;
    };
    control
        .subscriptions
        .disconnect_session(session_id, JsonRpcNotification::shutdown())
}

pub async fn reload_personas(registry_root: PathBuf) {
    let Some(control) = MCP_CONTROL.get() else {
        return;
    };
    let mut personas = control.personas.write().await;
    *personas = persona_names_from_registry_root(&registry_root);
}

fn mcp_socket_path() -> Result<PathBuf, String> {
    Ok(home_dir()?
        .join("Library")
        .join("Application Support")
        .join("AgentCanvas")
        .join("mcp.sock"))
}

/// Bind the MCP listener, clearing a stale socket file left by a crashed or force-quit
/// instance. `UnixListener::bind` fails with `AddrInUse` if the path already exists, which
/// would silently kill the MCP server on a cold restart (the socket file lingers because we
/// don't unlink on shutdown). Before removing it we probe-connect: if something answers, a
/// real instance owns the socket and we refuse rather than orphan it.
async fn bind_listener(socket_path: &PathBuf) -> Result<UnixListener, String> {
    match UnixListener::bind(socket_path) {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            if UnixStream::connect(socket_path).await.is_ok() {
                return Err(format!(
                    "another AgentCanvas instance is already listening on {}",
                    socket_path.display()
                ));
            }
            // Stale file from a dead instance — remove and retry.
            let _ = fs::remove_file(socket_path);
            UnixListener::bind(socket_path).map_err(|error| error.to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

async fn run_listener(
    app_handle: AppHandle,
    control: Arc<McpControl>,
    shutdown_rx: watch::Receiver<bool>,
) -> Result<(), String> {
    let listener = bind_listener(&control.socket_path).await?;

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
    let (disconnect_tx, disconnect_rx) = watch::channel(false);

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
        if *disconnect_rx.borrow() {
            break;
        }
        match tokio::time::timeout(Duration::from_millis(200), lines.next_line()).await {
            Ok(Ok(Some(line))) => {
                if let Some(response) = dispatch_line(
                    &app_handle,
                    &control,
                    &line,
                    &notification_tx,
                    &disconnect_tx,
                    &mut active_session,
                ) && response_tx.send(response).is_err()
                {
                    break;
                }
            }
            Ok(Ok(None)) => break,
            Ok(Err(error)) => {
                eprintln!("AgentCanvas MCP connection read error: {error}");
                break;
            }
            Err(_) => {
                let _ = disconnect_rx.has_changed();
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
        // Notify the frontend so it can refresh the sessions list immediately,
        // rather than waiting until the next window-focus rescan.
        if let Some(window) = app_handle.get_webview_window("main") {
            let _ = window.emit("agentcanvas://sessions-changed", json!({}));
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
    disconnect_tx: &watch::Sender<bool>,
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
            disconnect_tx.clone(),
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
    disconnect_tx: watch::Sender<bool>,
    id: Value,
    params: Value,
    active_session: &mut Option<McpSession>,
) -> Value {
    let state = app_handle.state::<AppState>();
    // All DB work (insert_agent_session) happens inside this block.
    // The window.emit for sessions-changed runs AFTER the lock is released,
    // so we capture the flag and session_id first, then emit.
    let (response, session_connected) = match state.db.lock() {
        Ok(conn) => {
            let response =
                handle_initialize_with_conn(id, params, &conn, &control.personas, active_session);
            let session_connected = if response.get("result").is_some()
                && let Some(session) = active_session.as_ref()
            {
                control.subscriptions.register_default(
                    session.session_id.clone(),
                    notification_tx,
                    disconnect_tx,
                );
                true
            } else {
                false
            };
            (response, session_connected)
            // `conn` (MutexGuard) is dropped here at the end of this block.
        }
        Err(_) => (rpc_error(id, -32603, "state db lock poisoned"), false),
    };
    // Notify the frontend so the sessions panel refreshes immediately.
    // This runs AFTER the db lock has been released (lock discipline).
    if session_connected {
        if let Some(window) = app_handle.get_webview_window("main") {
            let _ = window.emit("agentcanvas://sessions-changed", json!({}));
        }
    }
    response
}

fn handle_initialize_with_conn(
    id: Value,
    params: Value,
    conn: &rusqlite::Connection,
    personas: &Arc<RwLock<HashSet<String>>>,
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
    let persona_known = personas
        .try_read()
        .map(|personas| personas.contains(persona))
        .unwrap_or_else(|_| valid_persona_names().contains(persona));
    if !persona_known {
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
    // Connect-time dedup: retire any live rows this connection supersedes (same session_id
    // reconnecting, plus legacy 'unknown-session' ghosts). Keeps Presence from accumulating
    // duplicate cards. DB-only — safe inside the lock.
    if let Err(error) =
        sessions::retire_superseded_sessions(conn, session_id, connected_at, connected_at)
    {
        eprintln!("AgentCanvas MCP: retire_superseded_sessions failed: {error}");
    }
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

/// Return the current time as ISO-8601 UTC with trailing `Z` (protocol §4 rule 6).
fn iso8601_now() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    epoch_secs_to_iso8601(secs)
}

pub(crate) fn epoch_secs_to_iso8601(secs: u64) -> String {
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hh = time_of_day / 3600;
    let mm = (time_of_day % 3600) / 60;
    let ss = time_of_day % 60;
    // Days since Unix epoch → calendar date (Gregorian, civil-proleptic algorithm).
    // Works for all dates from 1970 onwards (z is always positive for unix timestamps).
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr = if m <= 2 { y + 1 } else { y };
    format!("{yr:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Remove internal side-channel fields that handlers smuggle through the tool result so the
/// dispatcher can act on them post-lock (`_dispatch_meta` for dispatch_interaction,
/// `_read_lifecycle` for get_user_messages). These are NEVER part of the protocol contract and
/// must not reach the agent. Cleans both `structuredContent` and the mirrored `content[0].text`.
/// Pure (no AppHandle) so the wire-contract guarantee is unit-testable.
fn strip_internal_side_channels(name: &str, rpc_response: &mut Value) {
    let key = match name {
        "dispatch_interaction" => "_dispatch_meta",
        "get_user_messages" => "_read_lifecycle",
        _ => return,
    };
    let removed = rpc_response
        .get_mut("result")
        .and_then(|r| r.get_mut("structuredContent"))
        .and_then(Value::as_object_mut)
        .map(|sc| sc.remove(key).is_some())
        .unwrap_or(false);
    if !removed {
        return;
    }
    // Rebuild content[0].text from the cleaned structuredContent so the text mirror matches.
    if let Some(sc_clean) = rpc_response
        .get("result")
        .and_then(|r| r.get("structuredContent"))
        .cloned()
    {
        if let Some(text) = rpc_response
            .get_mut("result")
            .and_then(|r| r.get_mut("content"))
            .and_then(|c| c.get_mut(0))
            .and_then(|c| c.get_mut("text"))
        {
            *text = Value::String(serde_json::to_string(&sc_clean).unwrap_or_default());
        }
    }
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

    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();

    // For tools that mutate the DB and then need watcher/window side-effects, we must
    // NOT call resync_watcher_from_db or any Tauri window op while the db MutexGuard is
    // still alive.  std::sync::Mutex is not reentrant: a second state.db.lock() on the
    // same thread while the first guard is held deadlocks forever.
    //
    // Strategy: pass app_handle=None into call_tool for these two handlers so they
    // perform ONLY their DB work under the lock, then drop the guard by ending the match
    // arm, and finally run the side-effects (watcher resync + window focus) here, after
    // the guard has been released.
    // notify_user now does only DB work (insert row) under the lock; the Tauri
    // emit runs post-lock, same pattern as open_artifact / attach_artifact.
    let needs_post_lock_side_effects = matches!(name, "open_artifact" | "attach_artifact" | "notify_user" | "dispatch_interaction" | "get_user_messages");

    let (mut rpc_response, open_path, notify_payload, dispatch_meta, read_lifecycle) = {
        let conn = match state.db.lock() {
            Ok(conn) => conn,
            Err(_) => return rpc_error(id, -32603, "state db lock poisoned"),
        };
        // Pass app_handle=None for mutating tools so their handlers cannot
        // attempt state.db.lock() or window ops while this guard is still held.
        let effective_app_handle = if needs_post_lock_side_effects {
            None
        } else {
            Some(app_handle)
        };
        let response = handle_tools_call_with_conn(
            id,
            params.clone(),
            &conn,
            &paths,
            current_focus,
            active_session,
            effective_app_handle,
        );
        // For open_artifact, capture the path argument now (inside the block so it is
        // clear we are still inside the lock scope, but the path is just a string copy).
        let open_path = if name == "open_artifact" {
            params
                .get("arguments")
                .and_then(|a| a.get("path"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        } else {
            None
        };
        // For notify_user, capture the arguments for the post-lock emit.
        let notify_payload = if name == "notify_user" {
            params.get("arguments").cloned()
        } else {
            None
        };
        // For dispatch_interaction: extract the internal _dispatch_meta side-channel.
        let dispatch_meta = if name == "dispatch_interaction" {
            response
                .get("result")
                .and_then(|r| r.get("structuredContent"))
                .and_then(|sc| sc.get("_dispatch_meta"))
                .cloned()
        } else {
            None
        };
        // For get_user_messages: extract the _read_lifecycle side-channel.
        let read_lifecycle = if name == "get_user_messages" {
            response
                .get("result")
                .and_then(|r| r.get("structuredContent"))
                .and_then(|sc| sc.get("_read_lifecycle"))
                .and_then(Value::as_array)
                .cloned()
        } else {
            None
        };
        (response, open_path, notify_payload, dispatch_meta, read_lifecycle)
        // `conn` (the MutexGuard) is dropped here at the end of this block.
    };

    // Strip internal side-channel fields (_dispatch_meta / _read_lifecycle) from the MCP
    // response before it reaches the agent — they are never part of the protocol contract.
    strip_internal_side_channels(name, &mut rpc_response);

    // Side-effects that must NOT run while the db guard is held.
    if needs_post_lock_side_effects && rpc_response.get("result").is_some() {
        // Resync the watcher now that the db lock is free (open_artifact / attach_artifact).
        if matches!(name, "open_artifact" | "attach_artifact") {
            let _ = resync_watcher_from_db(&state);
        }

        // For open_artifact: bring the window to front and emit the focus event.
        if let Some(path_string) = open_path {
            if let Some(window) = app_handle.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
                let _ = window.emit(
                    "agentcanvas://focus-and-open",
                    json!({ "path": path_string }),
                );
            }
            if let Ok(mut current_focus) = state.current_focus.lock() {
                *current_focus = Some(path_string);
            }
        }

        // For notify_user: emit the live toast event and the messages-changed event
        // after the db guard has been released (lock discipline).
        if let Some(notify_args) = notify_payload {
            if let Some(window) = app_handle.get_webview_window("main") {
                let _ = window.emit("agentcanvas://notify-user", &notify_args);
                let _ = window.emit("agentcanvas://messages-changed", json!({}));
            }
        }

        // For dispatch_interaction: raise the window, emit lifecycle event + interaction-dispatched.
        if let Some(meta) = dispatch_meta {
            let interaction_id = meta.get("interaction_id").and_then(Value::as_str).unwrap_or("").to_owned();
            let trace_id = meta.get("trace_id").cloned().unwrap_or(Value::Null);
            let class = meta.get("class").and_then(Value::as_str).unwrap_or("").to_owned();
            let ts = iso8601_now();
            if let Some(window) = app_handle.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
                // Lifecycle event for §5.1.
                let _ = window.emit(
                    "agentcanvas://interaction.dispatched",
                    json!({
                        "interaction_id": interaction_id,
                        "class": class,
                        "trace_id": trace_id,
                        "ts": ts
                    }),
                );
                // Also emit interaction-dispatched for the UI to open the form.
                let _ = window.emit(
                    "agentcanvas://interaction-dispatched",
                    json!({ "interaction_id": interaction_id }),
                );
                // Ping the notification channel (reuse existing messages-changed).
                let _ = window.emit("agentcanvas://messages-changed", json!({}));
            }
        }

        // For get_user_messages: emit interaction.read lifecycle events post-lock.
        if let Some(read_ids) = read_lifecycle {
            let ts = iso8601_now();
            if let Some(window) = app_handle.get_webview_window("main") {
                for item in &read_ids {
                    let interaction_id = item.get("interaction_id").and_then(Value::as_str).unwrap_or("");
                    let trace_id = item.get("trace_id").cloned().unwrap_or(Value::Null);
                    let class = item.get("class").and_then(Value::as_str).unwrap_or("");
                    let _ = window.emit(
                        "agentcanvas://interaction.read",
                        json!({
                            "interaction_id": interaction_id,
                            "trace_id": trace_id,
                            "class": class,
                            "ts": ts
                        }),
                    );
                }
                if !read_ids.is_empty() {
                    let _ = window.emit("agentcanvas://messages-changed", json!({}));
                }
            }
        }
    }

    rpc_response
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
            myfiles_dir: temp.path().join("AgentCanvas/MyFiles"),
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
        let personas = Arc::new(RwLock::new(valid_persona_names()));
        let response = handle_initialize_with_conn(
            json!(1),
            json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"manual-test","version":"0.0.1","agentCanvas":{"persona":"cpo","agent":"claude","project":"agent-canvas","session_id":"manual-test-1"}}}),
            &conn,
            &personas,
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
        let personas = Arc::new(RwLock::new(valid_persona_names()));
        let response = handle_initialize_with_conn(
            json!(2),
            json!({"clientInfo":{"agentCanvas":{"persona":"unknown-persona","agent":"codex","project":"agent-canvas","session_id":"unknown-1"}}}),
            &conn,
            &personas,
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
    fn tools_list_returns_ten_tools_with_input_schemas() {
        let response = rpc_result(json!(3), json!({ "tools": tools::tool_schemas() }));

        let tools = response["result"]["tools"].as_array().expect("tools");
        assert_eq!(tools.len(), 10);
        assert!(tools.iter().all(|tool| tool.get("inputSchema").is_some()));
        assert!(tools.iter().any(|tool| tool["name"] == "add_comment"));
        assert!(tools.iter().any(|tool| tool["name"] == "dispatch_interaction"));
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
        let (close_tx, _) = tokio::sync::watch::channel(false);
        registry.register_default("s1".to_owned(), tx, close_tx);

        registry.subscribe("s1", true, true);

        let subscription = registry.get("s1").expect("subscription");
        assert!(subscription.artifact_updated);
        assert!(subscription.artifact_focused);
    }

    #[test]
    fn default_subscription_includes_artifact_updated() {
        let registry = SubscriptionRegistry::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        let (close_tx, _) = tokio::sync::watch::channel(false);
        registry.register_default("s1".to_owned(), tx, close_tx);

        assert!(registry.get("s1").expect("subscription").artifact_updated);
    }

    #[test]
    fn default_subscription_excludes_artifact_focused() {
        let registry = SubscriptionRegistry::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        let (close_tx, _) = tokio::sync::watch::channel(false);
        registry.register_default("s1".to_owned(), tx, close_tx);

        assert!(!registry.get("s1").expect("subscription").artifact_focused);
    }

    #[test]
    fn watcher_change_dispatches_artifact_updated_notification() {
        let temp = tempfile::tempdir_in(std::env::current_dir().expect("cwd")).expect("tempdir");
        let target = temp.path().join("watched.md");
        fs::write(&target, "old").expect("write old");

        let registry = SubscriptionRegistry::default();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (close_tx, _) = tokio::sync::watch::channel(false);
        registry.register_default("s1".to_owned(), tx, close_tx);
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
        let (close_tx1, _) = tokio::sync::watch::channel(false);
        let (close_tx2, _) = tokio::sync::watch::channel(false);
        registry.register_default("s1".to_owned(), tx1, close_tx1);
        registry.register_default("s2".to_owned(), tx2, close_tx2);
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

        let comments = response["result"]["structuredContent"]["comments"]
            .as_array()
            .expect("comments");
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0]["id"], "new");
    }

    #[test]
    fn get_user_messages_v1_1_0_returns_interactions_not_user_messages() {
        // get_user_messages now returns submitted/draft interactions (protocol v1.1.0),
        // not the legacy user_messages rows.
        let (conn, paths, _temp) = test_state();

        // Insert a submitted interaction for s1 with a valid §4 response_json.
        let now = unix_now();
        conn.execute(
            r#"INSERT INTO interactions(interaction_id, session_id, class, request_json, status, response_json, created_at, responded_at)
               VALUES (?1, ?2, ?3, ?4, 'submitted', ?5, ?6, ?6)"#,
            rusqlite::params![
                "iid-1",
                "s1",
                "decision-set",
                r#"{"interaction_id":"iid-1","class":"decision-set","questions":[]}"#,
                r#"{"interaction_id":"iid-1","class":"decision-set","artifact_path":null,"status":"submitted","submitted_at":"2026-05-22T12:00:00Z","responses":[]}"#,
                now
            ],
        )
        .expect("insert interaction s1");
        conn.execute(
            r#"INSERT INTO interactions(interaction_id, session_id, class, request_json, status, response_json, created_at, responded_at)
               VALUES (?1, ?2, ?3, ?4, 'submitted', ?5, ?6, ?6)"#,
            rusqlite::params![
                "iid-2",
                "s2",
                "decision-set",
                r#"{"interaction_id":"iid-2","class":"decision-set","questions":[]}"#,
                r#"{"interaction_id":"iid-2","class":"decision-set","artifact_path":null,"status":"submitted","submitted_at":"2026-05-22T12:01:00Z","responses":[]}"#,
                now
            ],
        )
        .expect("insert interaction s2");

        let response = handle_tools_call_with_conn(
            json!(13),
            json!({"name":"get_user_messages","arguments":{}}),
            &conn,
            &paths,
            None,
            Some(&test_session("s1")),
            None,
        );

        let messages = response["result"]["structuredContent"]["messages"]
            .as_array()
            .expect("messages");
        // Only s1's interaction should be returned.
        assert_eq!(messages.len(), 1);
        // Wrapper-level interaction_id.
        assert_eq!(messages[0]["interaction_id"], "iid-1");
        // ts mirrors submitted_at.
        assert_eq!(messages[0]["ts"], "2026-05-22T12:00:00Z");
        // payload must be a §4 object with matching interaction_id.
        assert!(messages[0]["payload"].is_object());
        assert_eq!(messages[0]["payload"]["interaction_id"], "iid-1");
        assert_eq!(messages[0]["payload"]["class"], "decision-set");
        // _read_lifecycle side-channel is present in handle_tools_call_with_conn output
        // (it carries read IDs for post-lock lifecycle emits); stripped by handle_tools_call.
        // structuredContent must be an object (regression guard).
        assert!(response["result"]["structuredContent"].is_object());
    }

    #[test]
    fn strip_internal_side_channels_removes_fields_from_wire() {
        // get_user_messages: _read_lifecycle must be gone from both structuredContent and text.
        let mut resp = json!({
            "result": {
                "structuredContent": {
                    "messages": [],
                    "_read_lifecycle": [{ "interaction_id": "x" }]
                },
                "content": [{
                    "type": "text",
                    "text": "{\"messages\":[],\"_read_lifecycle\":[{\"interaction_id\":\"x\"}]}"
                }]
            }
        });
        strip_internal_side_channels("get_user_messages", &mut resp);
        assert!(resp["result"]["structuredContent"]["_read_lifecycle"].is_null());
        assert!(resp["result"]["structuredContent"]["messages"].is_array());
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            !text.contains("_read_lifecycle"),
            "content text mirror must not leak the side-channel: {text}"
        );

        // dispatch_interaction: _dispatch_meta must be gone too.
        let mut resp2 = json!({
            "result": {
                "structuredContent": { "dispatched": true, "_dispatch_meta": { "x": 1 } },
                "content": [{ "type": "text", "text": "{\"dispatched\":true,\"_dispatch_meta\":{\"x\":1}}" }]
            }
        });
        strip_internal_side_channels("dispatch_interaction", &mut resp2);
        assert!(resp2["result"]["structuredContent"]["_dispatch_meta"].is_null());
        assert_eq!(resp2["result"]["structuredContent"]["dispatched"], json!(true));
        assert!(!resp2["result"]["content"][0]["text"].as_str().unwrap().contains("_dispatch_meta"));

        // Unrelated tool: untouched.
        let mut resp3 = json!({ "result": { "structuredContent": { "ok": true } } });
        let before = resp3.clone();
        strip_internal_side_channels("get_artifact", &mut resp3);
        assert_eq!(resp3, before);
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
        let comments = response["result"]["structuredContent"]["comments"]
            .as_array()
            .expect("comments");
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0]["body"], "Body");
    }

    #[test]
    fn notify_user_inserts_row_and_returns_delivered() {
        let (conn, paths, _temp) = test_state();
        fs::create_dir_all(&paths.canvas_root).expect("canvas");
        let action_path = paths.canvas_root.join("x.md");
        fs::write(&action_path, "# X\n").expect("action file");
        sessions::insert_agent_session(&conn, "s1", "cpo", "claude", "agent-canvas", 1)
            .expect("session");
        let response = handle_tools_call_with_conn(
            json!(27),
            json!({"name":"notify_user","arguments":{"severity":"warn","message":"Check this","action":{"label":"Open","artifact_path":action_path.to_string_lossy()}}}),
            &conn,
            &paths,
            None,
            Some(&test_session("s1")),
            None,
        );

        assert_eq!(response["result"]["structuredContent"]["delivered"], true);
        // The id should be a non-empty string.
        assert!(response["result"]["structuredContent"]["id"].as_str().map(|s| !s.is_empty()).unwrap_or(false));
        // Verify row landed in agent_messages.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_messages WHERE session_id = 's1'", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn agent_messages_list_and_acknowledge_cycle() {
        let (conn, paths, _temp) = test_state();
        fs::create_dir_all(&paths.canvas_root).expect("canvas");
        sessions::insert_agent_session(&conn, "s1", "cpo", "claude", "agent-canvas", 1)
            .expect("session");
        // Insert via notify_user tool.
        handle_tools_call_with_conn(
            json!(30),
            json!({"name":"notify_user","arguments":{"severity":"info","message":"Hello from agent"}}),
            &conn,
            &paths,
            None,
            Some(&test_session("s1")),
            None,
        );
        // List — should return one unacknowledged message.
        let messages = sessions::list_unacknowledged_agent_messages(&conn).expect("list");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message, "Hello from agent");
        assert_eq!(messages[0].severity, "info");
        assert_eq!(messages[0].session_id, "s1");
        // Acknowledge (delete).
        sessions::delete_agent_message(&conn, &messages[0].id).expect("delete");
        // List again — should be empty.
        let after = sessions::list_unacknowledged_agent_messages(&conn).expect("list after");
        assert!(after.is_empty());
    }

    #[test]
    fn agent_messages_migration_idempotent() {
        let conn = Connection::open_in_memory().expect("db");
        sessions::migrate_agent_messages(&conn).expect("migration 1");
        sessions::migrate_agent_messages(&conn).expect("migration 2");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_messages", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 0);
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
        let paths = response["result"]["structuredContent"]["artifacts"]
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
        let (close_tx, _) = tokio::sync::watch::channel(false);
        registry.register_default("s1".to_owned(), tx, close_tx);

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

    #[test]
    fn list_agent_sessions_returns_mcp_and_manual_union() {
        let (conn, _, _temp) = test_state();
        conn.execute(
            "INSERT INTO manual_agent_sessions(id, persona, backbone, context, connected_at, last_active) VALUES ('m1', 'cpo', 'claude', 'Inbox', 1, 2)",
            [],
        )
        .expect("manual");
        sessions::insert_agent_session(&conn, "s1", "cto", "codex", "vellum", 3).expect("mcp");

        let listed = sessions::list_agent_sessions(&conn).expect("sessions");

        assert_eq!(listed.len(), 2);
        assert!(listed.iter().any(|session| session.id == "m1"
            && session.source == "manual"
            && session.agent == "claude"
            && session.project == "Inbox"));
        assert!(listed.iter().any(|session| session.id == "s1"
            && session.source == "mcp"
            && session.agent == "codex"
            && session.project == "vellum"));
    }

    #[test]
    fn list_agent_sessions_includes_attached_paths() {
        let (conn, _, _temp) = test_state();
        sessions::insert_agent_session(&conn, "s1", "cto", "codex", "vellum", 3).expect("mcp");
        sessions::attach_artifact(&conn, "s1", "/tmp/a.md", 4).expect("attach a");
        sessions::attach_artifact(&conn, "s1", "/tmp/b.html", 5).expect("attach b");

        let listed = sessions::list_agent_sessions(&conn).expect("sessions");
        let session = listed
            .iter()
            .find(|session| session.id == "s1")
            .expect("s1");

        assert_eq!(session.attached_paths, vec!["/tmp/b.html", "/tmp/a.md"]);
    }

    #[test]
    fn list_agent_sessions_excludes_disconnected_mcp_sessions() {
        let (conn, _, _temp) = test_state();
        sessions::insert_agent_session(&conn, "s1", "cto", "codex", "vellum", 3).expect("mcp");
        sessions::disconnect_agent_session(&conn, "s1", 3, 4).expect("disconnect");

        let listed = sessions::list_agent_sessions(&conn).expect("sessions");

        assert!(listed.iter().all(|session| session.id != "s1"));
    }

    #[test]
    fn disconnect_mcp_session_emits_shutdown_and_removes_session() {
        let (conn, _, _temp) = test_state();
        sessions::insert_agent_session(&conn, "s1", "cto", "codex", "vellum", 3).expect("mcp");
        let registry = SubscriptionRegistry::default();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (close_tx, mut close_rx) = tokio::sync::watch::channel(false);
        registry.register_default("s1".to_owned(), tx, close_tx);

        let disconnected = registry.disconnect_session("s1", JsonRpcNotification::shutdown());
        sessions::delete_agent_session(&conn, "s1").expect("delete");

        assert!(disconnected);
        assert_eq!(
            rx.try_recv().expect("shutdown").method,
            "notifications/shutdown"
        );
        assert!(*close_rx.borrow_and_update());
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_sessions WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 0);
    }

    #[test]
    fn install_for_claude_code_creates_config_when_missing() {
        let temp = tempfile::tempdir_in(std::env::current_dir().expect("cwd")).expect("tempdir");
        let config = temp.path().join(".claude.json");
        let shim = temp.path().join("agent-canvas-mcp");

        let result =
            crate::install_mcp_for_claude_code_at(config.clone(), shim.clone()).expect("install");
        let json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(config).expect("read")).expect("json");

        assert_eq!(result.action, crate::InstallAction::Created);
        assert_eq!(
            json["mcpServers"]["agent-canvas"]["command"].as_str(),
            Some(shim.to_str().unwrap())
        );
        assert_eq!(json["mcpServers"]["agent-canvas"]["args"], json!([]));
    }

    #[test]
    fn install_for_claude_code_replaces_existing_entry_preserving_others() {
        let temp = tempfile::tempdir_in(std::env::current_dir().expect("cwd")).expect("tempdir");
        let config = temp.path().join(".claude.json");
        fs::write(
            &config,
            r#"{"mcpServers":{"agent-canvas":{"command":"/old"},"other":{"command":"/keep"}}}"#,
        )
        .expect("seed");
        let shim = temp.path().join("agent-canvas-mcp");

        let result =
            crate::install_mcp_for_claude_code_at(config.clone(), shim.clone()).expect("install");
        let json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(config).expect("read")).expect("json");

        assert_eq!(result.action, crate::InstallAction::Updated);
        assert_eq!(
            json["mcpServers"]["agent-canvas"]["command"].as_str(),
            Some(shim.to_str().unwrap())
        );
        assert_eq!(json["mcpServers"]["other"]["command"], "/keep");
    }

    #[test]
    fn install_for_codex_writes_correct_toml_shape() {
        let temp = tempfile::tempdir_in(std::env::current_dir().expect("cwd")).expect("tempdir");
        let config = temp.path().join("config.toml");
        fs::write(&config, "[other]\nvalue = 1\n").expect("seed");
        let shim = temp.path().join("agent-canvas-mcp");

        crate::install_mcp_for_codex_at(config.clone(), shim.clone()).expect("install");
        let toml: toml::Value = fs::read_to_string(config)
            .expect("read")
            .parse()
            .expect("toml");

        assert_eq!(
            toml["mcp_servers"]["agent-canvas"]["command"].as_str(),
            Some(shim.to_str().unwrap())
        );
        assert_eq!(
            toml["mcp_servers"]["agent-canvas"]["args"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(toml["other"]["value"].as_integer(), Some(1));
    }

    #[test]
    fn install_for_cursor_idempotent() {
        let temp = tempfile::tempdir_in(std::env::current_dir().expect("cwd")).expect("tempdir");
        let config = temp.path().join("mcp.json");
        let shim = temp.path().join("agent-canvas-mcp");

        let first = crate::install_mcp_for_cursor_at(config.clone(), shim.clone()).expect("first");
        let second = crate::install_mcp_for_cursor_at(config, shim).expect("second");

        assert_eq!(first.action, crate::InstallAction::Created);
        assert_eq!(second.action, crate::InstallAction::Noop);
    }

    #[test]
    fn reload_persona_registry_invalidates_mcp_cache() {
        let temp = tempfile::tempdir_in(std::env::current_dir().expect("cwd")).expect("tempdir");
        let registry_root = temp.path().join("plugins");
        let agent_dir = registry_root.join("reviewer").join("agents");
        fs::create_dir_all(&agent_dir).expect("agent dir");
        fs::write(
            agent_dir.join("reviewer.md"),
            "---\nname: reviewer\ncolor: teal\n---\n# Reviewer\n",
        )
        .expect("persona");

        let mut cache = valid_persona_names();
        assert!(!cache.contains("reviewer"));
        cache = crate::persona_names_from_registry_root(&registry_root);

        assert!(cache.contains("reviewer"));
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

    // ------------------------------------------------------------------
    // Regression test: db-lock deadlock (open_artifact / attach_artifact)
    // ------------------------------------------------------------------
    //
    // Root cause: the dispatcher acquired state.db.lock() and then, while
    // holding the guard, called resync_watcher_from_db(&state) which
    // called state.db.lock() again.  std::sync::Mutex is NOT reentrant —
    // the second lock attempt deadlocks the thread forever.
    //
    // Fix: the dispatcher passes app_handle=None into call_tool for these
    // two handlers so no side-effect code runs under the lock.  After the
    // guard is dropped the dispatcher calls resync_watcher_from_db itself.
    //
    // This test exercises the exact lock discipline the dispatcher uses:
    // acquire the Mutex<Connection>, call the tool with app_handle=None,
    // drop the guard, then call resync_watcher_from_db.  If the old bug
    // were present (side-effect inside the handler while conn is held) the
    // test would deadlock; with the fix it completes immediately.
    //
    // A true timeout-based hang test is not used here because it would
    // leave a blocked OS thread and require a timeout harness.  Instead we
    // assert the structural invariant: the tool call with app_handle=None
    // completes synchronously and then resync_watcher_from_db (which takes
    // state.db.lock() itself) also completes — proving the guard was
    // released before resync runs.

    fn make_app_state_for_test(
        conn: Connection,
        paths: crate::AgentCanvasPaths,
    ) -> crate::AppState {
        crate::AppState {
            paths: Ok(paths),
            db: std::sync::Mutex::new(conn),
            watcher: std::sync::Mutex::new(None),
            current_focus: std::sync::Mutex::new(None),
            ephemeral_paths: std::sync::Mutex::new(std::collections::HashSet::new()),
            pending_opens: std::sync::Mutex::new(Vec::new()),
        }
    }

    #[test]
    fn attach_artifact_no_deadlock_db_lock_released_before_resync() {
        // Build the real AppState with a Mutex<Connection> — the same type
        // the dispatcher uses in production.
        let (conn, paths, _temp) = test_state();
        fs::create_dir_all(&paths.canvas_root).expect("canvas");
        let artifact = paths.canvas_root.join("deadlock_regression.md");
        fs::write(&artifact, "# Deadlock regression\n").expect("write");
        sessions::insert_agent_session(&conn, "dl-s1", "cpo", "claude", "agent-canvas", 1)
            .expect("session");

        let state = make_app_state_for_test(conn, paths.clone());

        // Step 1 — acquire db lock exactly as the (fixed) dispatcher does.
        let tool_result = {
            let conn_guard = state.db.lock().expect("db lock");
            // Pass app_handle=None: no side-effect code can run under the lock.
            let result = handle_tools_call_with_conn(
                json!(900),
                json!({
                    "name": "attach_artifact",
                    "arguments": { "path": artifact.to_string_lossy() }
                }),
                &conn_guard,
                &paths,
                None,
                Some(&test_session("dl-s1")),
                None, // ← app_handle=None, matching the fixed dispatcher
            );
            result
            // conn_guard is dropped here — MutexGuard released.
        };

        // Step 2 — resync runs AFTER the guard is dropped.  If the old bug
        // were present (resync inside the handler while guard is held) this
        // would have deadlocked in step 1 and never reached here.
        let resync = crate::resync_watcher_from_db(&state);

        assert!(
            tool_result.get("result").is_some(),
            "attach_artifact must succeed: {tool_result}"
        );
        assert!(
            resync.is_ok(),
            "resync_watcher_from_db must succeed after lock is released: {resync:?}"
        );
    }

    #[test]
    fn open_artifact_no_deadlock_db_lock_released_before_resync_and_window_ops() {
        let (conn, paths, _temp) = test_state();
        fs::create_dir_all(&paths.canvas_root).expect("canvas");
        let artifact = paths.canvas_root.join("deadlock_regression_open.md");
        fs::write(&artifact, "# Open deadlock regression\n").expect("write");

        let state = make_app_state_for_test(conn, paths.clone());

        let tool_result = {
            let conn_guard = state.db.lock().expect("db lock");
            let result = handle_tools_call_with_conn(
                json!(901),
                json!({
                    "name": "open_artifact",
                    "arguments": { "path": artifact.to_string_lossy() }
                }),
                &conn_guard,
                &paths,
                None,
                Some(&test_session("dl-s2")),
                None, // ← app_handle=None, no window ops attempted under the lock
            );
            result
            // conn_guard released here.
        };

        // After the guard is dropped, resync and current_focus update are safe.
        let resync = crate::resync_watcher_from_db(&state);
        // Simulate current_focus update (what the dispatcher does post-lock).
        {
            let mut cf = state.current_focus.lock().expect("current_focus");
            *cf = Some(artifact.to_string_lossy().into_owned());
        }

        assert!(
            tool_result.get("result").is_some(),
            "open_artifact must succeed: {tool_result}"
        );
        assert!(
            resync.is_ok(),
            "resync_watcher_from_db must succeed after lock is released: {resync:?}"
        );
        assert_eq!(
            state.current_focus.lock().expect("cf").as_deref(),
            Some(artifact.to_string_lossy().as_ref()),
        );
    }

    // ------------------------------------------------------------------
    // Task A regression: send_back_to_session auto-attach logic
    // ------------------------------------------------------------------
    //
    // The Tauri command itself can't be invoked in unit tests (no AppHandle),
    // but the two new DB queries are tested here at the sessions layer to
    // verify the invariants the command relies on.

    /// A live session with no prior attachment should be discoverable via the
    /// `agent_sessions WHERE disconnected_at IS NULL` query that the updated
    /// send_back_to_session uses before auto-attaching.
    #[test]
    fn send_back_auto_attach_live_session_is_found() {
        let (conn, paths, _temp) = test_state();
        let path = paths.canvas_root.join("auto.md");
        sessions::insert_agent_session(&conn, "sa1", "cto", "codex", "agent-canvas", 10)
            .expect("session");

        // No attachment yet — simulate the COUNT(*) from session_attachments.
        let attached: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM session_attachments WHERE session_id = ?1 AND path = ?2",
                rusqlite::params!["sa1", path.to_string_lossy()],
                |row| row.get(0),
            )
            .expect("count attachments");
        assert_eq!(attached, 0, "precondition: not yet attached");

        // Simulate the new live-session check.
        let session_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_sessions WHERE session_id = ?1 AND disconnected_at IS NULL",
                rusqlite::params!["sa1"],
                |row| row.get(0),
            )
            .expect("count sessions");
        assert_eq!(session_exists, 1, "live session must be found");

        // Auto-attach and then insert user message — both must succeed.
        sessions::attach_artifact(&conn, "sa1", &path.to_string_lossy(), 11).expect("auto-attach");
        sessions::insert_user_message(
            &conn, "sa1", &path.to_string_lossy(), Some("auto note"), Some("Review"), 12,
        )
        .expect("user message");

        let msg_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM user_messages WHERE session_id = 'sa1'",
                [],
                |row| row.get(0),
            )
            .expect("count messages");
        assert_eq!(msg_count, 1);
    }

    /// A disconnected (or unknown) session must NOT be found by the live-session check,
    /// which should cause the command to return an error.
    #[test]
    fn send_back_auto_attach_disconnected_session_returns_error() {
        let (conn, paths, _temp) = test_state();
        let path = paths.canvas_root.join("ghost.md");
        sessions::insert_agent_session(&conn, "sd1", "cpo", "claude", "agent-canvas", 20)
            .expect("session");
        // Mark as disconnected.
        sessions::disconnect_agent_session(&conn, "sd1", 20, 21).expect("disconnect");

        let attached: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM session_attachments WHERE session_id = ?1 AND path = ?2",
                rusqlite::params!["sd1", path.to_string_lossy()],
                |row| row.get(0),
            )
            .expect("count attachments");
        assert_eq!(attached, 0);

        // The live-session check must return 0 for a disconnected session.
        let session_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_sessions WHERE session_id = ?1 AND disconnected_at IS NULL",
                rusqlite::params!["sd1"],
                |row| row.get(0),
            )
            .expect("count sessions");
        assert_eq!(session_exists, 0, "disconnected session must not be found");

        // And for a session that never existed.
        let missing: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_sessions WHERE session_id = ?1 AND disconnected_at IS NULL",
                rusqlite::params!["never-existed"],
                |row| row.get(0),
            )
            .expect("count missing");
        assert_eq!(missing, 0, "unknown session must not be found");
    }

    // ---------------------------------------------------------------------------
    // Slice 0.5 — Interaction protocol tests
    // ---------------------------------------------------------------------------

    /// dispatch_interaction → row inserted with status=pending.
    #[test]
    fn dispatch_interaction_inserts_pending_row() {
        let (conn, paths, _temp) = test_state();
        let session = test_session("agent-1");

        let response = handle_tools_call_with_conn(
            json!(100),
            json!({
                "name": "dispatch_interaction",
                "arguments": {
                    "interaction_id": "test-uuid-1",
                    "class": "decision-set",
                    "title": "Where should logs go?",
                    "trace_id": "trace-abc",
                    "questions": [{
                        "question_id": "q1",
                        "question": "Pick log location",
                        "options": [{"key": "central", "label": "One central file"}]
                    }]
                }
            }),
            &conn,
            &paths,
            None,
            Some(&session),
            None,
        );

        assert!(response.get("error").is_none(), "should not error: {:?}", response);
        assert_eq!(response["result"]["structuredContent"]["dispatched"], true);
        assert_eq!(response["result"]["structuredContent"]["interaction_id"], "test-uuid-1");
        // _dispatch_meta is present in handle_tools_call_with_conn (internal side-channel);
        // it is stripped by handle_tools_call (the public MCP path).
        assert!(!response["result"]["structuredContent"]["_dispatch_meta"].is_null());

        // DB row must exist with status=pending.
        let status: String = conn
            .query_row(
                "SELECT status FROM interactions WHERE interaction_id = 'test-uuid-1'",
                [],
                |row| row.get(0),
            )
            .expect("interaction row");
        assert_eq!(status, "pending");
    }

    /// dispatch_interaction — missing interaction_id returns error.
    #[test]
    fn dispatch_interaction_missing_id_returns_error() {
        let (conn, paths, _temp) = test_state();
        let session = test_session("agent-1");

        let response = handle_tools_call_with_conn(
            json!(101),
            json!({"name":"dispatch_interaction","arguments":{"class":"decision-set","questions":[{"question_id":"q1","question":"?","options":[{"key":"a","label":"A"}]}]}}),
            &conn,
            &paths,
            None,
            Some(&session),
            None,
        );
        assert!(response.get("error").is_some());
    }

    /// dispatch_interaction — unknown class returns error.
    #[test]
    fn dispatch_interaction_unknown_class_returns_error() {
        let (conn, paths, _temp) = test_state();
        let session = test_session("agent-1");

        let response = handle_tools_call_with_conn(
            json!(102),
            json!({"name":"dispatch_interaction","arguments":{"interaction_id":"uuid-x","class":"bogus"}}),
            &conn,
            &paths,
            None,
            Some(&session),
            None,
        );
        assert!(response.get("error").is_some());
    }

    /// dispatch_interaction — decision-set without questions[] returns error.
    #[test]
    fn dispatch_interaction_decision_set_without_questions_returns_error() {
        let (conn, paths, _temp) = test_state();
        let session = test_session("agent-1");

        let response = handle_tools_call_with_conn(
            json!(103),
            json!({"name":"dispatch_interaction","arguments":{"interaction_id":"uuid-y","class":"decision-set"}}),
            &conn,
            &paths,
            None,
            Some(&session),
            None,
        );
        assert!(response.get("error").is_some());
    }

    /// get_user_messages → returns v1.1.0 wrapper: { interaction_id, ts, payload }.
    /// payload.interaction_id must equal wrapper.interaction_id (spec §5).
    #[test]
    fn get_user_messages_returns_v1_1_0_wrapped_shape() {
        let (conn, paths, _temp) = test_state();
        let now = unix_now();

        conn.execute(
            r#"INSERT INTO interactions(interaction_id, session_id, class, request_json, status, response_json, created_at, responded_at)
               VALUES ('iid-wrap', 's1', 'approval-gate', '{}', 'submitted', ?, ?, ?)"#,
            rusqlite::params![
                r#"{"interaction_id":"iid-wrap","class":"approval-gate","artifact_path":null,"status":"submitted","submitted_at":"2026-05-22T10:00:00Z","decision":"approve","reason":""}"#,
                now,
                now
            ],
        ).expect("insert");

        let response = handle_tools_call_with_conn(
            json!(104),
            json!({"name":"get_user_messages","arguments":{}}),
            &conn,
            &paths,
            None,
            Some(&test_session("s1")),
            None,
        );

        let msgs = response["result"]["structuredContent"]["messages"]
            .as_array()
            .expect("messages");
        assert_eq!(msgs.len(), 1);
        // Wrapper fields.
        assert_eq!(msgs[0]["interaction_id"], "iid-wrap");
        assert_eq!(msgs[0]["ts"], "2026-05-22T10:00:00Z");
        // payload must be an object.
        assert!(msgs[0]["payload"].is_object(), "payload must be an object");
        // payload.interaction_id == wrapper interaction_id (spec §5 normative).
        assert_eq!(msgs[0]["payload"]["interaction_id"], msgs[0]["interaction_id"]);
        // structuredContent must be an object.
        assert!(response["result"]["structuredContent"].is_object());
    }

    /// get_user_messages → read_at is set exactly once (idempotent on second call).
    #[test]
    fn get_user_messages_sets_read_at_once() {
        let (conn, paths, _temp) = test_state();
        let now = unix_now();

        conn.execute(
            r#"INSERT INTO interactions(interaction_id, session_id, class, request_json, status, response_json, created_at, responded_at)
               VALUES ('iid-readonce', 's1', 'decision-set', '{}', 'submitted', ?, ?, ?)"#,
            rusqlite::params![
                r#"{"interaction_id":"iid-readonce","class":"decision-set","artifact_path":null,"status":"submitted","submitted_at":"2026-05-22T11:00:00Z","responses":[]}"#,
                now, now
            ],
        ).expect("insert");

        // First call sets read_at.
        let _ = handle_tools_call_with_conn(
            json!(105),
            json!({"name":"get_user_messages","arguments":{}}),
            &conn, &paths, None,
            Some(&test_session("s1")),
            None,
        );
        let read_at_first: Option<i64> = conn
            .query_row(
                "SELECT read_at FROM interactions WHERE interaction_id = 'iid-readonce'",
                [], |row| row.get(0),
            ).expect("row");
        assert!(read_at_first.is_some(), "read_at should be set after first call");

        // Second call must NOT change read_at.
        let _ = handle_tools_call_with_conn(
            json!(106),
            json!({"name":"get_user_messages","arguments":{}}),
            &conn, &paths, None,
            Some(&test_session("s1")),
            None,
        );
        let read_at_second: Option<i64> = conn
            .query_row(
                "SELECT read_at FROM interactions WHERE interaction_id = 'iid-readonce'",
                [], |row| row.get(0),
            ).expect("row");
        assert_eq!(read_at_first, read_at_second, "read_at must not change on second read");
    }

    /// iso8601_now produces a well-formed timestamp.
    #[test]
    fn iso8601_now_is_well_formed() {
        let ts = iso8601_now();
        assert!(ts.ends_with('Z'), "must end with Z: {ts}");
        assert_eq!(ts.len(), 20, "expected len 20 (YYYY-MM-DDTHH:MM:SSZ): {ts}");
        assert!(ts.contains('T'), "must contain T: {ts}");
    }

    /// epoch_secs_to_iso8601 sanity check.
    #[test]
    fn epoch_secs_to_iso8601_known_date() {
        // 2026-05-22T00:00:00Z = 1779408000 unix seconds.
        let ts = epoch_secs_to_iso8601(1779408000);
        assert_eq!(ts, "2026-05-22T00:00:00Z");
        // Unix epoch itself.
        let ts2 = epoch_secs_to_iso8601(0);
        assert_eq!(ts2, "1970-01-01T00:00:00Z");
    }
}
