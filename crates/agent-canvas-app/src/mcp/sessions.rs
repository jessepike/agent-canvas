use std::{collections::HashMap, sync::Arc};

use parking_lot::Mutex;
use rusqlite::{Connection, params};
use serde::Serialize;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use super::notifications::JsonRpcNotification;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSession {
    pub session_id: String,
    pub persona: String,
    pub agent: String,
    pub project: String,
    pub connected_at: i64,
}

#[derive(Clone)]
pub struct Subscription {
    pub artifact_updated: bool,
    pub artifact_focused: bool,
    pub tx: mpsc::UnboundedSender<JsonRpcNotification>,
    pub disconnect_tx: watch::Sender<bool>,
}

#[derive(Clone, Default)]
pub struct SubscriptionRegistry {
    inner: Arc<Mutex<HashMap<String, Subscription>>>,
}

impl SubscriptionRegistry {
    pub fn register_default(
        &self,
        session_id: String,
        tx: mpsc::UnboundedSender<JsonRpcNotification>,
        disconnect_tx: watch::Sender<bool>,
    ) {
        self.inner.lock().insert(
            session_id,
            Subscription {
                artifact_updated: true,
                artifact_focused: false,
                tx,
                disconnect_tx,
            },
        );
    }

    pub fn remove(&self, session_id: &str) {
        self.inner.lock().remove(session_id);
    }

    pub fn subscribe(&self, session_id: &str, artifact_updated: bool, artifact_focused: bool) {
        if let Some(subscription) = self.inner.lock().get_mut(session_id) {
            subscription.artifact_updated = artifact_updated;
            subscription.artifact_focused = artifact_focused;
        }
    }

    #[cfg(test)]
    pub fn get(&self, session_id: &str) -> Option<Subscription> {
        self.inner.lock().get(session_id).cloned()
    }

    pub fn dispatch_artifact_updated(&self, notification: JsonRpcNotification) -> usize {
        self.dispatch(notification, |subscription| subscription.artifact_updated)
    }

    pub fn dispatch_artifact_focused(&self, notification: JsonRpcNotification) -> usize {
        self.dispatch(notification, |subscription| subscription.artifact_focused)
    }

    pub fn dispatch_all(&self, notification: JsonRpcNotification) -> usize {
        self.dispatch(notification, |_| true)
    }

    pub fn dispatch_to_session(&self, session_id: &str, notification: JsonRpcNotification) -> bool {
        let Some(tx) = self
            .inner
            .lock()
            .get(session_id)
            .map(|subscription| subscription.tx.clone())
        else {
            return false;
        };
        tx.send(notification).is_ok()
    }

    pub fn disconnect_session(&self, session_id: &str, notification: JsonRpcNotification) -> bool {
        let Some(subscription) = self.inner.lock().remove(session_id) else {
            return false;
        };
        let _ = subscription.tx.send(notification);
        let _ = subscription.disconnect_tx.send(true);
        true
    }

    fn dispatch(
        &self,
        notification: JsonRpcNotification,
        predicate: impl Fn(&Subscription) -> bool,
    ) -> usize {
        let targets = self
            .inner
            .lock()
            .iter()
            .filter_map(|(session_id, subscription)| {
                predicate(subscription).then(|| (session_id.clone(), subscription.tx.clone()))
            })
            .collect::<Vec<_>>();

        let mut sent = 0;
        let mut stale = Vec::new();
        for (session_id, tx) in targets {
            if tx.send(notification.clone()).is_ok() {
                sent += 1;
            } else {
                stale.push(session_id);
            }
        }
        if !stale.is_empty() {
            let mut subscriptions = self.inner.lock();
            for session_id in stale {
                subscriptions.remove(&session_id);
            }
        }
        sent
    }
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

pub const USER_MESSAGES_MIGRATION_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS user_messages (
  id           TEXT PRIMARY KEY,
  session_id   TEXT NOT NULL,
  path         TEXT NOT NULL,
  note         TEXT,
  action_verb  TEXT,
  created_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_user_messages_session ON user_messages(session_id);
CREATE INDEX IF NOT EXISTS idx_user_messages_created ON user_messages(created_at);
"#;

pub const AGENT_MESSAGES_MIGRATION_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS agent_messages (
  id                   TEXT PRIMARY KEY,
  session_id           TEXT NOT NULL,
  severity             TEXT NOT NULL,
  message              TEXT NOT NULL,
  action_artifact_path TEXT,
  action_label         TEXT,
  created_at           INTEGER NOT NULL,
  acknowledged_at      INTEGER
);
CREATE INDEX IF NOT EXISTS idx_agent_messages_ack ON agent_messages(acknowledged_at);
"#;

pub const INTERACTIONS_MIGRATION_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS interactions (
  interaction_id  TEXT PRIMARY KEY,
  session_id      TEXT NOT NULL,
  class           TEXT NOT NULL,
  title           TEXT,
  artifact_path   TEXT,
  artifact_inline TEXT,
  trace_id        TEXT,
  request_json    TEXT NOT NULL,
  status          TEXT NOT NULL DEFAULT 'pending',
  response_json   TEXT,
  created_at      INTEGER NOT NULL,
  responded_at    INTEGER,
  read_at         INTEGER
);
CREATE INDEX IF NOT EXISTS idx_interactions_session_status ON interactions(session_id, status);
CREATE INDEX IF NOT EXISTS idx_interactions_id ON interactions(interaction_id);
"#;

pub const SESSION_ATTACHMENTS_MIGRATION_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS session_attachments (
  session_id  TEXT NOT NULL,
  path        TEXT NOT NULL,
  attached_at INTEGER NOT NULL,
  PRIMARY KEY (session_id, path)
);
CREATE INDEX IF NOT EXISTS idx_session_attachments_path ON session_attachments(path);
"#;

#[derive(Debug, Clone, Serialize)]
pub struct SessionAttachment {
    pub session_id: String,
    pub persona: String,
    pub agent: String,
    pub project: String,
    pub attached_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentSession {
    pub id: String,
    pub source: String,
    pub persona: String,
    pub agent: String,
    pub project: String,
    pub connected_at: i64,
    pub last_active: Option<i64>,
    pub is_live: bool,
    pub attached_paths: Vec<String>,
}

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

pub fn migrate_user_messages(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(USER_MESSAGES_MIGRATION_SQL)
        .map_err(|error| error.to_string())
}

pub fn migrate_agent_messages(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(AGENT_MESSAGES_MIGRATION_SQL)
        .map_err(|error| error.to_string())
}

pub fn migrate_session_attachments(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(SESSION_ATTACHMENTS_MIGRATION_SQL)
        .map_err(|error| error.to_string())
}

pub fn migrate_interactions(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(INTERACTIONS_MIGRATION_SQL)
        .map_err(|error| error.to_string())
}

// ---------------------------------------------------------------------------
// Interaction struct + DB helpers
// ---------------------------------------------------------------------------

/// A dispatched interaction row, used for list_interactions / get_interaction.
#[derive(Debug, Clone, Serialize)]
pub struct Interaction {
    pub interaction_id: String,
    pub session_id: String,
    pub class: String,
    pub title: Option<String>,
    pub artifact_path: Option<String>,
    pub artifact_inline: Option<String>,
    pub trace_id: Option<String>,
    pub request_json: String,
    pub status: String,
    pub response_json: Option<String>,
    pub created_at: i64,
    pub responded_at: Option<i64>,
    pub read_at: Option<i64>,
}

pub fn insert_interaction(
    conn: &Connection,
    interaction_id: &str,
    session_id: &str,
    class: &str,
    title: Option<&str>,
    artifact_path: Option<&str>,
    artifact_inline: Option<&str>,
    trace_id: Option<&str>,
    request_json: &str,
    created_at: i64,
) -> Result<(), String> {
    conn.execute(
        r#"
        INSERT INTO interactions(
          interaction_id, session_id, class, title, artifact_path, artifact_inline,
          trace_id, request_json, status, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', ?9)
        "#,
        params![
            interaction_id,
            session_id,
            class,
            title,
            artifact_path,
            artifact_inline,
            trace_id,
            request_json,
            created_at
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn get_interaction(conn: &Connection, interaction_id: &str) -> Result<Option<Interaction>, String> {
    let result = conn.query_row(
        r#"
        SELECT interaction_id, session_id, class, title, artifact_path, artifact_inline,
               trace_id, request_json, status, response_json, created_at, responded_at, read_at
        FROM interactions WHERE interaction_id = ?1
        "#,
        params![interaction_id],
        |row| {
            Ok(Interaction {
                interaction_id: row.get(0)?,
                session_id: row.get(1)?,
                class: row.get(2)?,
                title: row.get(3)?,
                artifact_path: row.get(4)?,
                artifact_inline: row.get(5)?,
                trace_id: row.get(6)?,
                request_json: row.get(7)?,
                status: row.get(8)?,
                response_json: row.get(9)?,
                created_at: row.get(10)?,
                responded_at: row.get(11)?,
                read_at: row.get(12)?,
            })
        },
    );
    match result {
        Ok(row) => Ok(Some(row)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn list_interactions_pending(conn: &Connection) -> Result<Vec<Interaction>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT interaction_id, session_id, class, title, artifact_path, artifact_inline,
                   trace_id, request_json, status, response_json, created_at, responded_at, read_at
            FROM interactions
            WHERE status IN ('pending', 'draft')
            ORDER BY created_at ASC, interaction_id ASC
            "#,
        )
        .map_err(|error| error.to_string())?;
    stmt.query_map([], |row| {
        Ok(Interaction {
            interaction_id: row.get(0)?,
            session_id: row.get(1)?,
            class: row.get(2)?,
            title: row.get(3)?,
            artifact_path: row.get(4)?,
            artifact_inline: row.get(5)?,
            trace_id: row.get(6)?,
            request_json: row.get(7)?,
            status: row.get(8)?,
            response_json: row.get(9)?,
            created_at: row.get(10)?,
            responded_at: row.get(11)?,
            read_at: row.get(12)?,
        })
    })
    .map_err(|error| error.to_string())?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|error| error.to_string())
}

/// Mark `read_at` for interactions not yet read. Returns the updated rows.
/// Called inside a DB lock; emits must happen post-lock.
pub fn set_interactions_read_at(
    conn: &Connection,
    session_id: &str,
    now_ts: i64,
) -> Result<Vec<(String, Option<String>, String)>, String> {
    // Fetch rows that will be updated (read_at IS NULL).
    let mut stmt = conn
        .prepare(
            "SELECT interaction_id, trace_id, class FROM interactions WHERE session_id = ?1 AND status IN ('submitted','draft') AND read_at IS NULL"
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, Option<String>, String)> = stmt
        .query_map(params![session_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    // Update them.
    conn.execute(
        "UPDATE interactions SET read_at = ?2 WHERE session_id = ?1 AND status IN ('submitted','draft') AND read_at IS NULL",
        params![session_id, now_ts],
    )
    .map_err(|e| e.to_string())?;
    Ok(rows)
}

pub fn get_interactions_submitted_for_session(
    conn: &Connection,
    session_id: &str,
    since_epoch: Option<i64>,
) -> Result<Vec<Interaction>, String> {
    let mut sql = String::from(
        r#"
        SELECT interaction_id, session_id, class, title, artifact_path, artifact_inline,
               trace_id, request_json, status, response_json, created_at, responded_at, read_at
        FROM interactions
        WHERE session_id = ?1 AND status IN ('submitted','draft')
        "#,
    );
    let mut values: Vec<rusqlite::types::Value> = vec![session_id.to_owned().into()];
    if let Some(since) = since_epoch {
        sql.push_str(" AND responded_at >= ?2");
        values.push(since.into());
    }
    sql.push_str(" ORDER BY responded_at ASC, interaction_id ASC");
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    stmt.query_map(rusqlite::params_from_iter(values), |row| {
        Ok(Interaction {
            interaction_id: row.get(0)?,
            session_id: row.get(1)?,
            class: row.get(2)?,
            title: row.get(3)?,
            artifact_path: row.get(4)?,
            artifact_inline: row.get(5)?,
            trace_id: row.get(6)?,
            request_json: row.get(7)?,
            status: row.get(8)?,
            response_json: row.get(9)?,
            created_at: row.get(10)?,
            responded_at: row.get(11)?,
            read_at: row.get(12)?,
        })
    })
    .map_err(|e| e.to_string())?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| e.to_string())
}

/// Set interaction status + response + responded_at. Returns (trace_id, class) for lifecycle emit.
pub fn submit_interaction(
    conn: &Connection,
    interaction_id: &str,
    status: &str,
    response_json: &str,
    responded_at: i64,
) -> Result<Option<(String, Option<String>)>, String> {
    let result = conn.query_row(
        "SELECT trace_id, class FROM interactions WHERE interaction_id = ?1",
        params![interaction_id],
        |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
    );
    match result {
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e.to_string()),
        Ok(_) => {}
    }
    let (trace_id, class) = result.unwrap();
    conn.execute(
        "UPDATE interactions SET status = ?2, response_json = ?3, responded_at = ?4 WHERE interaction_id = ?1",
        params![interaction_id, status, response_json, responded_at],
    )
    .map_err(|e| e.to_string())?;
    Ok(Some((class, trace_id)))
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

pub fn attach_artifact(
    conn: &Connection,
    session_id: &str,
    path: &str,
    attached_at: i64,
) -> Result<(), String> {
    conn.execute(
        r#"
        INSERT INTO session_attachments(session_id, path, attached_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(session_id, path) DO UPDATE SET attached_at = excluded.attached_at
        "#,
        params![session_id, path, attached_at],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn cleanup_session_attachments(conn: &Connection, session_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM session_attachments WHERE session_id = ?1",
        params![session_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn delete_agent_session(conn: &Connection, session_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM agent_sessions WHERE session_id = ?1 AND disconnected_at IS NULL",
        params![session_id],
    )
    .map_err(|error| error.to_string())?;
    cleanup_session_attachments(conn, session_id)?;
    Ok(())
}

pub fn list_agent_sessions(conn: &Connection) -> Result<Vec<AgentSession>, String> {
    let mut sessions = Vec::new();
    let mut manual = conn
        .prepare(
            r#"
            SELECT id, persona, backbone, COALESCE(context, ''), connected_at, last_active
            FROM manual_agent_sessions
            "#,
        )
        .map_err(|error| error.to_string())?;
    let manual_rows = manual
        .query_map([], |row| {
            Ok(AgentSession {
                id: row.get(0)?,
                source: "manual".to_owned(),
                persona: row.get(1)?,
                agent: row.get(2)?,
                project: row.get(3)?,
                connected_at: row.get(4)?,
                last_active: Some(row.get(5)?),
                is_live: false,
                attached_paths: Vec::new(),
            })
        })
        .map_err(|error| error.to_string())?;
    for row in manual_rows {
        sessions.push(row.map_err(|error| error.to_string())?);
    }

    let mut mcp = conn
        .prepare(
            r#"
            SELECT session_id, source, persona, agent, project, connected_at
            FROM agent_sessions
            WHERE disconnected_at IS NULL
            "#,
        )
        .map_err(|error| error.to_string())?;
    let mcp_rows = mcp
        .query_map([], |row| {
            Ok(AgentSession {
                id: row.get(0)?,
                source: row.get(1)?,
                persona: row.get(2)?,
                agent: row.get(3)?,
                project: row.get(4)?,
                connected_at: row.get(5)?,
                last_active: None,
                is_live: true,
                attached_paths: Vec::new(),
            })
        })
        .map_err(|error| error.to_string())?;
    for row in mcp_rows {
        let mut session = row.map_err(|error| error.to_string())?;
        session.attached_paths = attached_paths_for_session(conn, &session.id)?;
        sessions.push(session);
    }

    sessions.sort_by(|left, right| {
        let left_time = left.last_active.unwrap_or(left.connected_at);
        let right_time = right.last_active.unwrap_or(right.connected_at);
        right_time
            .cmp(&left_time)
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(sessions)
}

fn attached_paths_for_session(conn: &Connection, session_id: &str) -> Result<Vec<String>, String> {
    let mut statement = conn
        .prepare(
            r#"
            SELECT path
            FROM session_attachments
            WHERE session_id = ?1
            ORDER BY attached_at DESC, path ASC
            "#,
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map(params![session_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn attachments_for_path(
    conn: &Connection,
    path: &str,
) -> Result<Vec<SessionAttachment>, String> {
    let mut statement = conn
        .prepare(
            r#"
            SELECT a.session_id, s.persona, s.agent, s.project, a.attached_at
            FROM session_attachments a
            JOIN agent_sessions s ON s.session_id = a.session_id
            WHERE a.path = ?1
              AND s.connected_at = (
                SELECT MAX(s2.connected_at)
                FROM agent_sessions s2
                WHERE s2.session_id = a.session_id
              )
            ORDER BY a.attached_at DESC, a.session_id ASC
            "#,
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map(params![path], |row| {
            Ok(SessionAttachment {
                session_id: row.get(0)?,
                persona: row.get(1)?,
                agent: row.get(2)?,
                project: row.get(3)?,
                attached_at: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn insert_user_message(
    conn: &Connection,
    session_id: &str,
    path: &str,
    note: Option<&str>,
    action_verb: Option<&str>,
    created_at: i64,
) -> Result<String, String> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        r#"
        INSERT INTO user_messages(id, session_id, path, note, action_verb, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![id, session_id, path, note, action_verb, created_at],
    )
    .map_err(|error| error.to_string())?;
    Ok(id)
}

/// A persisted agent→user notification message.
#[derive(Debug, Clone, Serialize)]
pub struct AgentMessage {
    pub id: String,
    pub session_id: String,
    pub persona: String,
    pub agent: String,
    pub severity: String,
    pub message: String,
    pub action_artifact_path: Option<String>,
    pub action_label: Option<String>,
    pub created_at: i64,
}

/// Insert a new agent message row. Returns the generated id.
pub fn insert_agent_message(
    conn: &Connection,
    session_id: &str,
    severity: &str,
    message: &str,
    action_artifact_path: Option<&str>,
    action_label: Option<&str>,
    created_at: i64,
) -> Result<String, String> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        r#"
        INSERT INTO agent_messages(id, session_id, severity, message, action_artifact_path, action_label, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![id, session_id, severity, message, action_artifact_path, action_label, created_at],
    )
    .map_err(|error| error.to_string())?;
    Ok(id)
}

/// List all unacknowledged agent messages, newest first.
/// Joins agent_sessions to surface persona/agent for display.
pub fn list_unacknowledged_agent_messages(conn: &Connection) -> Result<Vec<AgentMessage>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT
              m.id,
              m.session_id,
              COALESCE(
                (SELECT s.persona FROM agent_sessions s
                 WHERE s.session_id = m.session_id AND s.disconnected_at IS NULL
                 LIMIT 1),
                ''
              ) AS persona,
              COALESCE(
                (SELECT s.agent FROM agent_sessions s
                 WHERE s.session_id = m.session_id AND s.disconnected_at IS NULL
                 LIMIT 1),
                ''
              ) AS agent,
              m.severity,
              m.message,
              m.action_artifact_path,
              m.action_label,
              m.created_at
            FROM agent_messages m
            WHERE m.acknowledged_at IS NULL
            ORDER BY m.created_at DESC, m.id DESC
            "#,
        )
        .map_err(|error| error.to_string())?;
    stmt.query_map([], |row| {
        Ok(AgentMessage {
            id: row.get(0)?,
            session_id: row.get(1)?,
            persona: row.get(2)?,
            agent: row.get(3)?,
            severity: row.get(4)?,
            message: row.get(5)?,
            action_artifact_path: row.get(6)?,
            action_label: row.get(7)?,
            created_at: row.get(8)?,
        })
    })
    .map_err(|error| error.to_string())?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|error| error.to_string())
}

/// Delete (acknowledge) an agent message by id. Delete-on-ack keeps the table small.
pub fn delete_agent_message(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM agent_messages WHERE id = ?1",
        params![id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Startup ghost-session sweep (Slice 8).
///
/// Mark every MCP session whose `disconnected_at` is NULL as disconnected
/// at `now_ts`. No MCP connection can survive an app restart, so any session
/// still "live" in the DB is a stale ghost left by a previous force-quit or
/// crash. Call this once during `initialize_state_db`.
pub fn disconnect_all_sessions(conn: &Connection, now_ts: i64) -> Result<usize, String> {
    let count = conn
        .execute(
            "UPDATE agent_sessions SET disconnected_at = ?1 WHERE disconnected_at IS NULL",
            params![now_ts],
        )
        .map_err(|error| error.to_string())?;
    Ok(count)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(AGENT_SESSIONS_MIGRATION_SQL).expect("migrate agent_sessions");
        conn
    }

    /// disconnect_all_sessions sweeps every NULL disconnected_at row.
    #[test]
    fn test_disconnect_all_sessions_clears_live_ghosts() {
        let conn = in_memory_db();
        let now: i64 = 1_700_000_000;

        // Insert two "live" MCP sessions (disconnected_at IS NULL).
        conn.execute(
            "INSERT INTO agent_sessions(session_id, source, persona, agent, project, connected_at) VALUES (?1, 'mcp', 'claude', 'claude', 'Default', ?2)",
            params!["s1", now - 100],
        )
        .expect("insert s1");
        conn.execute(
            "INSERT INTO agent_sessions(session_id, source, persona, agent, project, connected_at) VALUES (?1, 'mcp', 'codex', 'codex', 'Default', ?2)",
            params!["s2", now - 50],
        )
        .expect("insert s2");

        // Before sweep: both are live.
        let live_before: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_sessions WHERE disconnected_at IS NULL",
                [],
                |row| row.get(0),
            )
            .expect("count before");
        assert_eq!(live_before, 2, "expect 2 live sessions before sweep");

        // Run the sweep.
        let swept = disconnect_all_sessions(&conn, now).expect("sweep");
        assert_eq!(swept, 2, "expect 2 rows updated");

        // After sweep: none are live.
        let live_after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_sessions WHERE disconnected_at IS NULL",
                [],
                |row| row.get(0),
            )
            .expect("count after");
        assert_eq!(live_after, 0, "expect 0 live sessions after sweep");

        // Disconnected_at is set to now for both.
        let dc_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_sessions WHERE disconnected_at = ?1",
                params![now],
                |row| row.get(0),
            )
            .expect("count dc");
        assert_eq!(dc_count, 2, "expect both rows stamped with now_ts");
    }

    /// disconnect_all_sessions is a no-op when there are no live sessions.
    #[test]
    fn test_disconnect_all_sessions_noop_when_empty() {
        let conn = in_memory_db();
        let now: i64 = 1_700_000_001;

        let swept = disconnect_all_sessions(&conn, now).expect("sweep empty");
        assert_eq!(swept, 0, "expect 0 rows updated on empty table");
    }

    /// Already-disconnected rows are not touched by a second sweep.
    #[test]
    fn test_disconnect_all_sessions_skips_already_disconnected() {
        let conn = in_memory_db();
        let then: i64 = 1_700_000_100;
        let now: i64 = 1_700_000_200;

        // Insert a session that was already disconnected at `then`.
        conn.execute(
            "INSERT INTO agent_sessions(session_id, source, persona, agent, project, connected_at, disconnected_at) VALUES (?1, 'mcp', 'claude', 'claude', 'Default', ?2, ?3)",
            params!["s_old", then - 500, then],
        )
        .expect("insert already-disconnected");

        let swept = disconnect_all_sessions(&conn, now).expect("sweep");
        assert_eq!(swept, 0, "already-disconnected row must not be touched");

        // Confirm original disconnected_at is unchanged.
        let dc: i64 = conn
            .query_row(
                "SELECT disconnected_at FROM agent_sessions WHERE session_id = 's_old'",
                [],
                |row| row.get(0),
            )
            .expect("read dc");
        assert_eq!(dc, then, "original disconnected_at must be preserved");
    }
}
