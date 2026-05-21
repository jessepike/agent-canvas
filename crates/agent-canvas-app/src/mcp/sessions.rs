use rusqlite::{Connection, params};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSession {
    pub session_id: String,
    pub persona: String,
    pub agent: String,
    pub project: String,
    pub connected_at: i64,
}

pub const AGENT_SESSIONS_MIGRATION_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS agent_sessions (
  session_id      TEXT NOT NULL,
  source          TEXT NOT NULL,
  persona         TEXT NOT NULL,
  agent           TEXT NOT NULL,
  project         TEXT NOT NULL,
  connected_at    INTEGER NOT NULL,
  disconnected_at INTEGER,
  PRIMARY KEY (session_id, connected_at)
);
"#;

pub fn migrate_manual_agent_sessions_if_needed(conn: &Connection) -> Result<(), String> {
    let columns = table_columns(conn, "agent_sessions")?;
    if columns.iter().any(|column| column == "backbone") {
        conn.execute_batch(
            r#"
            ALTER TABLE agent_sessions RENAME TO manual_agent_sessions;
            "#,
        )
        .map_err(|error| error.to_string())?;
    }

    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS manual_agent_sessions (
          id TEXT PRIMARY KEY,
          persona TEXT NOT NULL,
          backbone TEXT NOT NULL,
          context TEXT,
          connected_at INTEGER NOT NULL,
          last_active INTEGER NOT NULL
        );
        "#,
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

pub fn migrate_agent_sessions(conn: &Connection) -> Result<(), String> {
    migrate_manual_agent_sessions_if_needed(conn)?;
    conn.execute_batch(AGENT_SESSIONS_MIGRATION_SQL)
        .map_err(|error| error.to_string())
}

pub fn insert_agent_session(
    conn: &Connection,
    session_id: &str,
    persona: &str,
    agent: &str,
    project: &str,
    connected_at: i64,
) -> Result<McpSession, String> {
    conn.execute(
        r#"
        INSERT INTO agent_sessions(session_id, source, persona, agent, project, connected_at)
        VALUES (?1, 'mcp', ?2, ?3, ?4, ?5)
        "#,
        params![session_id, persona, agent, project, connected_at],
    )
    .map_err(|error| error.to_string())?;

    Ok(McpSession {
        session_id: session_id.to_owned(),
        persona: persona.to_owned(),
        agent: agent.to_owned(),
        project: project.to_owned(),
        connected_at,
    })
}

pub fn disconnect_agent_session(
    conn: &Connection,
    session_id: &str,
    connected_at: i64,
    disconnected_at: i64,
) -> Result<(), String> {
    conn.execute(
        r#"
        UPDATE agent_sessions
        SET disconnected_at = ?3
        WHERE session_id = ?1 AND connected_at = ?2 AND disconnected_at IS NULL
        "#,
        params![session_id, connected_at, disconnected_at],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn table_columns(conn: &Connection, table: &str) -> Result<Vec<String>, String> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| error.to_string())?;
    statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}
