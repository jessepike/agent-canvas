use std::{
    env,
    path::PathBuf,
    process::Command,
    process::ExitCode,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};
use tokio::{
    io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    time::sleep,
};

fn socket_path() -> Result<PathBuf, String> {
    let home = env::var_os("HOME").ok_or_else(|| "HOME is not set".to_owned())?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("AgentCanvas")
        .join("mcp.sock"))
}

async fn connect_with_launch(path: &PathBuf) -> Result<UnixStream, String> {
    if let Ok(stream) = UnixStream::connect(path).await {
        return Ok(stream);
    }

    Command::new("open")
        .args(["-a", "AgentCanvas.app"])
        .spawn()
        .map_err(|error| format!("failed to launch AgentCanvas.app: {error}"))?;

    for _ in 0..25 {
        sleep(Duration::from_millis(200)).await;
        if let Ok(stream) = UnixStream::connect(path).await {
            return Ok(stream);
        }
    }

    Err(format!(
        "AgentCanvas did not create MCP socket within 5s: {}",
        path.display()
    ))
}

/// Build the `clientInfo.agentCanvas` identity block this shim injects on `initialize`.
///
/// Claude Code (and most stdio MCP clients) cannot set `clientInfo.agentCanvas` themselves,
/// so without injection every connection lands as `default·unknown·unknown-session` — and
/// because they all share one `session_id`, repeated connects pile up as identical ghost
/// cards in AgentCanvas. The shim fixes this at the source:
///   - persona  ← $AGENTCANVAS_PERSONA      (fallback "default")
///   - agent    ← $AGENTCANVAS_AGENT        (fallback "claude" — this shim bridges Claude Code)
///   - project  ← $AGENTCANVAS_PROJECT or basename($PWD)  (fallback "default")
///   - session_id ← $AGENTCANVAS_SESSION_ID or a unique per-process id
///
/// The per-process `session_id` guarantees each shim connection is distinct and cleanly
/// disconnectable, instead of colliding on the shared "unknown-session" key.
fn build_identity() -> Value {
    let env_nonempty = |key: &str| env::var(key).ok().filter(|value| !value.is_empty());

    let persona = env_nonempty("AGENTCANVAS_PERSONA").unwrap_or_else(|| "default".to_owned());
    let agent = env_nonempty("AGENTCANVAS_AGENT").unwrap_or_else(|| "claude".to_owned());
    let project = env_nonempty("AGENTCANVAS_PROJECT")
        .or_else(|| {
            env::current_dir()
                .ok()
                .and_then(|dir| dir.file_name().map(|name| name.to_string_lossy().into_owned()))
                .filter(|name| !name.is_empty())
        })
        .unwrap_or_else(|| "default".to_owned());
    let session_id = env_nonempty("AGENTCANVAS_SESSION_ID").unwrap_or_else(|| {
        let pid = std::process::id();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|delta| delta.as_nanos())
            .unwrap_or(0);
        format!("{agent}-{pid}-{nanos}")
    });

    json!({
        "persona": persona,
        "agent": agent,
        "project": project,
        "session_id": session_id,
    })
}

/// If `line` is an `initialize` request without a `clientInfo.agentCanvas` block, inject one.
/// Returns `(possibly_rewritten_line, was_initialize)`. Any non-initialize line, unparseable
/// line, or one that already carries `agentCanvas` is returned verbatim. An explicit client
/// identity is never overridden.
fn maybe_inject(line: &str, identity: &Value) -> (String, bool) {
    let mut parsed: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) => return (line.to_owned(), false),
    };
    if parsed.get("method").and_then(Value::as_str) != Some("initialize") {
        return (line.to_owned(), false);
    }
    let Some(root) = parsed.as_object_mut() else {
        return (line.to_owned(), true);
    };
    let params = root
        .entry("params")
        .or_insert_with(|| json!({}));
    let Some(params_obj) = params.as_object_mut() else {
        return (line.to_owned(), true);
    };
    let client_info = params_obj
        .entry("clientInfo")
        .or_insert_with(|| json!({}));
    let Some(client_info_obj) = client_info.as_object_mut() else {
        return (line.to_owned(), true);
    };
    if !client_info_obj.contains_key("agentCanvas") {
        client_info_obj.insert("agentCanvas".to_owned(), identity.clone());
    }
    (parsed.to_string(), true)
}

async fn forward_stdio(stream: UnixStream) -> Result<(), String> {
    let (socket_read, mut socket_write) = stream.into_split();
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut stdin_lines = BufReader::new(stdin).lines();
    let mut socket_lines = BufReader::new(socket_read).lines();

    let identity = build_identity();
    let stdin_to_socket = async {
        let mut injected = false;
        while let Some(line) = stdin_lines.next_line().await? {
            let out_line = if injected {
                line
            } else {
                let (rewritten, was_initialize) = maybe_inject(&line, &identity);
                injected = was_initialize;
                rewritten
            };
            socket_write.write_all(out_line.as_bytes()).await?;
            socket_write.write_all(b"\n").await?;
            socket_write.flush().await?;
        }
        Ok::<(), io::Error>(())
    };

    let socket_to_stdout = async move {
        while let Some(line) = socket_lines.next_line().await? {
            stdout.write_all(line.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
        Ok::<(), io::Error>(())
    };

    let stdout_task = tokio::spawn(socket_to_stdout);
    stdin_to_socket.await.map_err(|error| error.to_string())?;
    stdout_task
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;

    Ok(())
}

fn write_startup_error(message: &str) {
    let response = json!({
        "jsonrpc": "2.0",
        "id": null,
        "error": {
            "code": -32000,
            "message": message
        }
    });
    println!("{response}");
}

fn main() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            write_startup_error(&format!("failed to start MCP shim runtime: {error}"));
            return ExitCode::from(1);
        }
    };
    runtime.block_on(async_main())
}

async fn async_main() -> ExitCode {
    let path = match socket_path() {
        Ok(path) => path,
        Err(error) => {
            write_startup_error(&error);
            return ExitCode::from(2);
        }
    };

    let stream = match connect_with_launch(&path).await {
        Ok(stream) => stream,
        Err(error) => {
            write_startup_error(&error);
            return ExitCode::from(2);
        }
    };

    match forward_stdio(stream).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            write_startup_error(&format!("MCP stdio bridge failed: {error}"));
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> Value {
        json!({
            "persona": "cto",
            "agent": "claude",
            "project": "agent-canvas",
            "session_id": "claude-123-456",
        })
    }

    #[test]
    fn injects_agent_canvas_when_absent() {
        let line = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"claude"}}}"#;
        let (out, was_init) = maybe_inject(line, &identity());
        assert!(was_init, "initialize must be detected");
        let parsed: Value = serde_json::from_str(&out).unwrap();
        let ac = &parsed["params"]["clientInfo"]["agentCanvas"];
        assert_eq!(ac["persona"], "cto");
        assert_eq!(ac["session_id"], "claude-123-456");
        // The original clientInfo.name is preserved alongside the injection.
        assert_eq!(parsed["params"]["clientInfo"]["name"], "claude");
    }

    #[test]
    fn creates_params_and_client_info_when_missing() {
        let line = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;
        let (out, was_init) = maybe_inject(line, &identity());
        assert!(was_init);
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["params"]["clientInfo"]["agentCanvas"]["agent"], "claude");
    }

    #[test]
    fn never_overrides_explicit_identity() {
        let line = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"agentCanvas":{"persona":"cpo","agent":"codex","project":"x","session_id":"explicit-1"}}}}"#;
        let (out, was_init) = maybe_inject(line, &identity());
        assert!(was_init);
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["params"]["clientInfo"]["agentCanvas"]["session_id"], "explicit-1");
        assert_eq!(parsed["params"]["clientInfo"]["agentCanvas"]["persona"], "cpo");
    }

    #[test]
    fn passes_non_initialize_lines_verbatim() {
        let line = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"open_artifact"}}"#;
        let (out, was_init) = maybe_inject(line, &identity());
        assert!(!was_init, "tools/call must not be treated as initialize");
        assert_eq!(out, line, "non-initialize lines must pass through unchanged");
    }

    #[test]
    fn passes_unparseable_lines_verbatim() {
        let line = "not json at all";
        let (out, was_init) = maybe_inject(line, &identity());
        assert!(!was_init);
        assert_eq!(out, line);
    }

    #[test]
    fn build_identity_has_all_fields() {
        let id = build_identity();
        for key in ["persona", "agent", "project", "session_id"] {
            assert!(id.get(key).and_then(Value::as_str).is_some(), "missing {key}");
        }
    }
}
