use std::{env, path::PathBuf, process::Command, process::ExitCode, time::Duration};

use serde_json::json;
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

async fn forward_stdio(stream: UnixStream) -> Result<(), String> {
    let (socket_read, mut socket_write) = stream.into_split();
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut stdin_lines = BufReader::new(stdin).lines();
    let mut socket_lines = BufReader::new(socket_read).lines();

    let stdin_to_socket = async {
        while let Some(line) = stdin_lines.next_line().await? {
            socket_write.write_all(line.as_bytes()).await?;
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
